//! End-to-end client-cost benchmark for redis-rs `MultiplexedConnection`.
//!
//! Fixed operation count, closed loop, `T` tasks sharing clones of ONE multiplexed connection.
//! Reports wall time, throughput and client process CPU (user+sys from getrusage) per operation.
//! With `--features count`, also reports heap allocations per operation (separate build, so the
//! counting allocator never perturbs the timing runs).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use redis::AsyncCommands;

#[cfg(feature = "count")]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    pub static ALLOCS: AtomicU64 = AtomicU64::new(0);
    pub static BYTES: AtomicU64 = AtomicU64::new(0);
    pub struct Counting;
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(l.size() as u64, Relaxed);
            unsafe { System.alloc(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(n as u64, Relaxed);
            unsafe { System.realloc(p, l, n) }
        }
    }
}
#[cfg(feature = "mimalloc")]
#[global_allocator]
static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static DHAT: dhat::Alloc = dhat::Alloc;

#[cfg(feature = "count")]
#[global_allocator]
static GLOBAL: counting::Counting = counting::Counting;

fn cpu_time() -> Duration {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
    tv(ru.ru_utime) + tv(ru.ru_stime)
}
fn cpu_split() -> (f64, f64) {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    (tv(ru.ru_utime), tv(ru.ru_stime))
}

struct Args {
    host: String,
    workload: String,
    tasks: usize,
    ops: usize,
    threads: usize,
    timeout_ms: Option<u64>, // None = library default; 0 = disabled
    label: String,
    resp3: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        host: std::env::var("BENCH_HOST").unwrap_or_else(|_| "127.0.0.1:6379".into()),
        workload: "get".into(),
        tasks: 16,
        ops: 200_000,
        threads: 4,
        timeout_ms: None,
        label: "base".into(),
        resp3: false,
    };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let val = v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--workload" => a.workload = val,
            "--tasks" => a.tasks = val.parse().unwrap(),
            "--ops" => a.ops = val.parse().unwrap(),
            "--threads" => a.threads = val.parse().unwrap(),
            "--timeout-ms" => a.timeout_ms = Some(val.parse().unwrap()),
            "--label" => a.label = val,
            "--resp3" => {
                a.resp3 = true;
                i += 1;
                continue;
            }
            "--host" => a.host = val,
            x => panic!("unknown arg {x}"),
        }
        i += 2;
    }
    a
}

const KEY: &str = "bench:k20";
const KEY1K: &str = "bench:k1k";
const HASH: &str = "bench:h50";

async fn setup(con: &mut redis::aio::MultiplexedConnection) {
    let _: () = con.set(KEY, "x".repeat(20)).await.unwrap();
    let _: () = con.set(KEY1K, "y".repeat(1024)).await.unwrap();
    for i in 0..100 {
        let _: () = con.set(format!("bench:m:{i:03}"), format!("value-{i:014}")).await.unwrap();
    }
    let fields: Vec<(String, String)> =
        (0..50).map(|i| (format!("field:{i:04}"), format!("value-{i:014}"))).collect();
    let _: () = con.del(HASH).await.unwrap();
    let _: () = con.hset_multiple(HASH, &fields).await.unwrap();
}

/// One logical operation. Returns a checksum contribution so nothing is optimised away and the
/// result is fully consumed into the type a typical application would use.
async fn one_op(
    con: &mut redis::aio::MultiplexedConnection,
    w: &str,
    mkeys: &[String],
    i: usize,
) -> usize {
    match w {
        "get" => {
            let v: Option<String> = con.get(KEY).await.unwrap();
            v.map_or(0, |s| s.len())
        }
        "getbytes" => {
            let v: Option<Vec<u8>> = con.get(KEY).await.unwrap();
            v.map_or(0, |s| s.len())
        }
        "getdirect" => {
            // Inherent method: bypasses the boxed future of `aio::ConnectionLike`.
            let mut c = redis::cmd("GET");
            c.arg(KEY);
            let v = con.send_packed_command(&c).await.unwrap();
            let v: Option<String> = redis::from_redis_value(v.extract_error().unwrap()).unwrap();
            v.map_or(0, |s| s.len())
        }
        "set" => {
            let _: () = con.set(KEY, "abcdefghijklmnopqrst").await.unwrap();
            1
        }
        "incr" => {
            let v: i64 = con.incr("bench:ctr", 1).await.unwrap();
            (v & 1) as usize
        }
        "get1k" => {
            let v: Option<String> = con.get(KEY1K).await.unwrap();
            v.map_or(0, |s| s.len())
        }
        "mget100" => {
            let v: Vec<String> = con.mget(mkeys).await.unwrap();
            v.iter().map(|s| s.len()).sum()
        }
        "hgetall" => {
            let v: HashMap<String, String> = con.hgetall(HASH).await.unwrap();
            v.len()
        }
        "pipe100" => {
            let mut p = redis::pipe();
            for k in mkeys {
                p.get(k);
            }
            let v: Vec<String> = p.query_async(con).await.unwrap();
            v.iter().map(|s| s.len()).sum()
        }
        "setkey" => {
            // distinct key per op: exercises argument formatting/encoding
            let _: () = con.set(&mkeys[i % mkeys.len()], i).await.unwrap();
            1
        }
        x => panic!("unknown workload {x}"),
    }
}

fn ops_per_logical(w: &str) -> usize {
    match w {
        "pipe100" => 100,
        _ => 1,
    }
}

fn main() {
    let args = parse_args();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.threads)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(run(args));
}

async fn run(a: Args) {
    let proto = if a.resp3 { "?protocol=resp3" } else { "" };
    let client = redis::Client::open(format!("redis://{}/{}", a.host, proto)).unwrap();
    let mut cfg = redis::AsyncConnectionConfig::new();
    if let Some(ms) = a.timeout_ms {
        cfg = cfg.set_response_timeout(if ms == 0 { None } else { Some(Duration::from_millis(ms)) });
    }
    let mut con = client.get_multiplexed_async_connection_with_config(&cfg).await.unwrap();
    setup(&mut con).await;
    let mkeys: Vec<String> = (0..100).map(|i| format!("bench:m:{i:03}")).collect();

    // warmup
    let warm = (a.ops / 10).max(a.tasks);
    drive(&con, &a, &mkeys, warm).await;

    #[cfg(feature = "count")]
    let (a0, b0) = (
        counting::ALLOCS.load(std::sync::atomic::Ordering::Relaxed),
        counting::BYTES.load(std::sync::atomic::Ordering::Relaxed),
    );
    let (u0, s0) = cpu_split();
    let c0 = cpu_time();
    let t0 = Instant::now();
    #[cfg(feature = "dhat-heap")]
    let profiler = dhat::Profiler::builder().file_name(format!("prof/dhat-{}-{}.json", a.label, a.workload)).build();
    let checksum = drive(&con, &a, &mkeys, a.ops).await;
    #[cfg(feature = "dhat-heap")]
    drop(profiler);
    let wall = t0.elapsed();
    let cpu = cpu_time() - c0;
    let (u1, s1) = cpu_split();
    #[cfg(feature = "count")]
    let (allocs, abytes) = (
        counting::ALLOCS.load(std::sync::atomic::Ordering::Relaxed) - a0,
        counting::BYTES.load(std::sync::atomic::Ordering::Relaxed) - b0,
    );
    #[cfg(not(feature = "count"))]
    let (allocs, abytes) = (0u64, 0u64);

    let cmds = a.ops * ops_per_logical(&a.workload);
    println!(
        "{{\"label\":\"{}\",\"workload\":\"{}\",\"tasks\":{},\"threads\":{},\"resp3\":{},\"timeout_ms\":{},\"ops\":{},\"cmds\":{},\"wall_s\":{:.4},\"ops_per_s\":{:.0},\"cpu_us_per_op\":{:.3},\"user_us_per_op\":{:.3},\"sys_us_per_op\":{:.3},\"allocs_per_op\":{:.2},\"alloc_bytes_per_op\":{:.0},\"checksum\":{}}}",
        a.label,
        a.workload,
        a.tasks,
        a.threads,
        a.resp3,
        a.timeout_ms.map_or("null".to_string(), |v| v.to_string()),
        a.ops,
        cmds,
        wall.as_secs_f64(),
        a.ops as f64 / wall.as_secs_f64(),
        cpu.as_secs_f64() * 1e6 / a.ops as f64,
        (u1 - u0) * 1e6 / a.ops as f64,
        (s1 - s0) * 1e6 / a.ops as f64,
        allocs as f64 / a.ops as f64,
        abytes as f64 / a.ops as f64,
        checksum
    );
}

async fn drive(
    con: &redis::aio::MultiplexedConnection,
    a: &Args,
    mkeys: &[String],
    ops: usize,
) -> usize {
    let per = ops / a.tasks;
    let mut hs = Vec::with_capacity(a.tasks);
    for t in 0..a.tasks {
        let mut c = con.clone();
        let w = a.workload.clone();
        let mk = mkeys.to_vec();
        hs.push(tokio::spawn(async move {
            let mut sum = 0usize;
            for i in 0..per {
                sum = sum.wrapping_add(one_op(&mut c, &w, &mk, t * per + i).await);
            }
            sum
        }));
    }
    let mut s = 0usize;
    for h in hs {
        s = s.wrapping_add(h.await.unwrap());
    }
    s
}
