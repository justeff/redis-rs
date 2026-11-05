//! Upper-bound microbenchmarks for 2.0 API ideas (single thread, no server).
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { ALLOCS.fetch_add(1, Relaxed); unsafe { System.alloc(l) } }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { unsafe { System.dealloc(p, l) } }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { ALLOCS.fetch_add(1, Relaxed); unsafe { System.realloc(p, l, n) } }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn bench<R>(name: &str, iters: u64, mut f: impl FnMut() -> R) -> f64 {
    for _ in 0..iters / 10 { black_box(f()); }
    let mut best = f64::MAX;
    let mut allocs = 0.0;
    for _ in 0..7 {
        let a0 = ALLOCS.load(Relaxed);
        let t0 = Instant::now();
        for _ in 0..iters { black_box(f()); }
        let ns = t0.elapsed().as_nanos() as f64 / iters as f64;
        allocs = (ALLOCS.load(Relaxed) - a0) as f64 / iters as f64;
        best = best.min(ns);
    }
    println!("{name:<44} {best:>9.1} ns/op  {allocs:>7.1} allocs/op");
    best
}

// ---- hand-written direct decoders (stand-in for a "decode straight from the frame" API) ----
fn line(buf: &[u8], pos: &mut usize) -> (u8, i64) {
    let kind = buf[*pos];
    let mut i = *pos + 1;
    let mut neg = false;
    let mut n: i64 = 0;
    if buf[i] == b'-' { neg = true; i += 1; }
    while buf[i] != b'\r' { n = n * 10 + (buf[i] - b'0') as i64; i += 1; }
    *pos = i + 2;
    (kind, if neg { -n } else { n })
}
fn bulk_str(buf: &[u8], pos: &mut usize) -> String {
    let (_, n) = line(buf, pos);
    let s = std::str::from_utf8(&buf[*pos..*pos + n as usize]).unwrap().to_owned();
    *pos += n as usize + 2;
    s
}
fn direct_vec_string(buf: &[u8]) -> Vec<String> {
    let mut pos = 0;
    let (_, n) = line(buf, &mut pos);
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n { out.push(bulk_str(buf, &mut pos)); }
    out
}
fn direct_map(buf: &[u8]) -> HashMap<String, String> {
    let mut pos = 0;
    let (kind, n) = line(buf, &mut pos);
    let pairs = if kind == b'%' { n } else { n / 2 } as usize;
    let mut out = HashMap::with_capacity(pairs);
    for _ in 0..pairs { let k = bulk_str(buf, &mut pos); let v = bulk_str(buf, &mut pos); out.insert(k, v); }
    out
}

fn main() {
    // MGET 100 x 20-byte values
    let mut mget = format!("*100\r\n").into_bytes();
    for i in 0..100 { let v = format!("value-{i:014}"); mget.extend(format!("${}\r\n{}\r\n", v.len(), v).bytes()); }
    // HGETALL 50 fields (RESP2 flat array)
    let mut hgetall = format!("*100\r\n").into_bytes();
    for i in 0..50 { for s in [format!("field:{i:04}"), format!("value-{i:014}")] { hgetall.extend(format!("${}\r\n{}\r\n", s.len(), s).bytes()); } }
    let get = b"$20\r\nxxxxxxxxxxxxxxxxxxxx\r\n".to_vec();

    println!("== decode: today (frame -> Value -> T) vs direct (frame -> T)");
    let a = bench("MGET100 -> Vec<String>  via Value", 200_000, || {
        let v = redis::parse_redis_value(&mget).unwrap();
        redis::from_redis_value::<Vec<String>>(v).unwrap()
    });
    let b = bench("MGET100 -> Vec<String>  direct", 200_000, || direct_vec_string(&mget));
    println!("   -> direct saves {:.0}%", (1.0 - b / a) * 100.0);
    let a = bench("HGETALL50 -> HashMap  via Value", 200_000, || {
        let v = redis::parse_redis_value(&hgetall).unwrap();
        redis::from_redis_value::<HashMap<String, String>>(v).unwrap()
    });
    let b = bench("HGETALL50 -> HashMap  direct", 200_000, || direct_map(&hgetall));
    println!("   -> direct saves {:.0}%", (1.0 - b / a) * 100.0);
    let a = bench("GET -> Option<String>  via Value", 5_000_000, || {
        let v = redis::parse_redis_value(&get).unwrap();
        redis::from_redis_value::<Option<String>>(v).unwrap()
    });
    let b = bench("GET -> Option<String>  direct", 5_000_000, || { let mut p = 0; bulk_str(&get, &mut p) });
    println!("   -> direct saves {:.0}%", (1.0 - b / a) * 100.0);

    println!("== encode: Cmd builder + get_packed_command vs direct write into a reused buffer");
    let key = "bench:k20";
    let val = "abcdefghijklmnopqrst";
    let a = bench("SET k v: cmd().arg().arg() + get_packed_command", 5_000_000, || {
        let mut c = redis::cmd("SET");
        c.arg(key).arg(val);
        c.get_packed_command()
    });
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let b = bench("SET k v: direct RESP into reused Vec", 5_000_000, || {
        buf.clear();
        let mut it = itoa::Buffer::new();
        buf.extend_from_slice(b"*3\r\n$3\r\nSET\r\n");
        for a in [key.as_bytes(), val.as_bytes()] {
            buf.push(b'$'); buf.extend_from_slice(it.format(a.len()).as_bytes()); buf.extend_from_slice(b"\r\n");
            buf.extend_from_slice(a); buf.extend_from_slice(b"\r\n");
        }
        buf.len()
    });
    println!("   -> direct saves {:.0}%", (1.0 - b / a) * 100.0);
    let keys: Vec<String> = (0..100).map(|i| format!("bench:m:{i:03}")).collect();
    let a = bench("MGET 100 keys: cmd().arg(&keys) + packed", 500_000, || {
        let mut c = redis::cmd("MGET");
        c.arg(&keys);
        c.get_packed_command()
    });
    let b = bench("MGET 100 keys: direct RESP into reused Vec", 500_000, || {
        buf.clear();
        let mut it = itoa::Buffer::new();
        buf.extend_from_slice(b"*101\r\n$4\r\nMGET\r\n");
        for a in &keys {
            buf.push(b'$'); buf.extend_from_slice(it.format(a.len()).as_bytes()); buf.extend_from_slice(b"\r\n");
            buf.extend_from_slice(a.as_bytes()); buf.extend_from_slice(b"\r\n");
        }
        buf.len()
    });
    println!("   -> direct saves {:.0}%", (1.0 - b / a) * 100.0);
}
