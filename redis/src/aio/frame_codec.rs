//! A codec for the multiplexed connection's driver that only finds reply boundaries.
//!
//! Aggregate replies (arrays, sets, maps) are handed to the caller as an undecoded `Bytes` frame and
//! decoded on the caller's task. That moves the per-element allocations of a large reply off the
//! single driver task, so they are allocated and freed on the same thread and decoded in parallel.
//! Scalars, errors, pushes and attributed replies are decoded by the driver, which needs to inspect
//! them (or for which a frame would cost more than it saves).

use bytes::{Buf, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::errors::{ParsingError, RedisError};
use crate::parser::parse_redis_value;
use crate::types::{RedisResult, Value};

const MAX_RECURSE_DEPTH: usize = 100;

/// A reply as produced by the driver.
pub(crate) enum Reply {
    /// A reply the driver already decoded.
    Value(Value),
    /// A complete, structurally validated RESP aggregate, decoded by the caller.
    Frame(Bytes),
    /// The replies of a pipeline.
    Many(Vec<Self>),
}

impl Reply {
    pub(crate) fn into_value(self) -> RedisResult<Value> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Frame(bytes) => parse_redis_value(&bytes),
            Self::Many(replies) => {
                let mut values = Vec::with_capacity(replies.len());
                for reply in replies {
                    values.push(reply.into_value()?);
                }
                Ok(Value::Array(values))
            }
        }
    }
}

/// Incremental RESP frame scanner. Keeps its position and the remaining child counts of the open
/// aggregates across calls, so a reply that arrives in many reads is scanned once.
#[derive(Default)]
pub(crate) struct FrameCodec {
    pos: usize,
    stack: Vec<u64>,
}

fn protocol_error(msg: &'static str) -> RedisError {
    RedisError::from(ParsingError::from(msg))
}

fn parse_len(line: &[u8]) -> RedisResult<i64> {
    let (neg, digits) = match line.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, line),
    };
    if digits.is_empty() || digits.len() > 18 {
        return Err(protocol_error("invalid length"));
    }
    let mut n: i64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return Err(protocol_error("invalid length"));
        }
        n = n * 10 + (b - b'0') as i64;
    }
    Ok(if neg { -n } else { n })
}

impl FrameCodec {
    fn finish(&mut self, src: &mut BytesMut) -> RedisResult<Reply> {
        let len = std::mem::take(&mut self.pos);
        debug_assert!(self.stack.is_empty());
        match src[0] {
            b'*' | b'~' | b'%' | b'$' => Ok(Reply::Frame(src.split_to(len).freeze())),
            _ => {
                let value = parse_redis_value(&src[..len]);
                src.advance(len);
                value.map(Reply::Value)
            }
        }
    }
}

impl Decoder for FrameCodec {
    type Item = Reply;
    type Error = RedisError;

    fn decode(&mut self, src: &mut BytesMut) -> RedisResult<Option<Reply>> {
        loop {
            let buf = &src[..];
            let start = self.pos;
            if start >= buf.len() {
                return Ok(None);
            }
            let Some(lf) = memchr::memchr(b'\n', &buf[start + 1..]) else {
                return Ok(None);
            };
            let lf = start + 1 + lf;
            if buf[lf - 1] != b'\r' || lf < start + 2 {
                return Err(protocol_error("line without CRLF"));
            }
            let line = &buf[start + 1..lf - 1];
            let after = lf + 1;
            let complete = match buf[start] {
                b'+' | b'-' | b':' | b',' | b'#' | b'_' | b'(' => {
                    self.pos = after;
                    true
                }
                b'$' | b'!' | b'=' => {
                    let n = parse_len(line)?;
                    if n < 0 {
                        self.pos = after;
                    } else {
                        let end = after
                            .checked_add(n as usize + 2)
                            .ok_or_else(|| protocol_error("length overflow"))?;
                        if buf.len() < end {
                            // Make room for the whole payload at once.
                            src.reserve(end - buf.len());
                            return Ok(None);
                        }
                        if &buf[end - 2..end] != b"\r\n" {
                            return Err(protocol_error("bulk without CRLF"));
                        }
                        self.pos = end;
                    }
                    true
                }
                kind @ (b'*' | b'~' | b'>' | b'%' | b'|') => {
                    let n = parse_len(line)?;
                    self.pos = after;
                    let children = match kind {
                        _ if n < 0 => 0,
                        b'%' => n as u64 * 2,
                        b'|' => n as u64 * 2 + 1,
                        _ => n as u64,
                    };
                    if children == 0 {
                        true
                    } else {
                        if self.stack.len() >= MAX_RECURSE_DEPTH {
                            return Err(protocol_error("Maximum recursion depth exceeded"));
                        }
                        self.stack.push(children);
                        false
                    }
                }
                _ => return Err(protocol_error("invalid RESP type byte")),
            };
            if complete {
                // Close every aggregate that this element completes.
                loop {
                    match self.stack.last_mut() {
                        None => return self.finish(src).map(Some),
                        Some(remaining) => {
                            *remaining -= 1;
                            if *remaining == 0 {
                                self.stack.pop();
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> RedisResult<Option<Reply>> {
        match self.decode(buf)? {
            Some(reply) => Ok(Some(reply)),
            None if buf.is_empty() => Ok(None),
            None => Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into()),
        }
    }
}

impl Encoder<Vec<u8>> for FrameCodec {
    type Error = RedisError;
    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(item.as_ref());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(input: &[u8], chunk: usize) -> Vec<Value> {
        let mut codec = FrameCodec::default();
        let mut buf = BytesMut::new();
        let mut out = Vec::new();
        for piece in input.chunks(chunk) {
            buf.extend_from_slice(piece);
            while let Some(reply) = codec.decode(&mut buf).unwrap() {
                out.push(reply.into_value().unwrap());
            }
        }
        assert!(buf.is_empty());
        out
    }

    #[test]
    fn frames_match_the_parser_for_any_split() {
        let input: &[u8] = b"+OK\r\n:12\r\n$3\r\nfoo\r\n$-1\r\n*-1\r\n*0\r\n*2\r\n$1\r\na\r\n*2\r\n:1\r\n+x\r\n%2\r\n+a\r\n:1\r\n+b\r\n*1\r\n_\r\n~1\r\n#t\r\n|1\r\n+k\r\n+v\r\n$2\r\nhi\r\n>2\r\n+message\r\n+m\r\n-ERR boom\r\n,1.5\r\n=7\r\ntxt:abc\r\n";
        let mut expected = Vec::new();
        let mut reference = crate::parser::ValueCodec::default();
        let mut whole = BytesMut::from(input);
        while let Some(value) = reference.decode(&mut whole).unwrap() {
            expected.push(value);
        }
        assert_eq!(expected.len(), 14);
        for chunk in 1..input.len() {
            assert_eq!(decode_all(input, chunk), expected, "chunk {chunk}");
        }
    }

    #[test]
    fn rejects_excessive_nesting() {
        let mut input = Vec::new();
        for _ in 0..(MAX_RECURSE_DEPTH + 1) {
            input.extend_from_slice(b"*1\r\n");
        }
        input.extend_from_slice(b":1\r\n");
        let mut codec = FrameCodec::default();
        let mut buf = BytesMut::from(&input[..]);
        assert!(codec.decode(&mut buf).is_err());
    }
}
