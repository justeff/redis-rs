[package]
name = "rrbench"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
redis = { path = "REDIS_PATH", features = ["tokio-comp"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "net"] }
libc = "0.2"
dhat = { version = "0.3", optional = true }
mimalloc = { version = "0.1", optional = true }

[features]
count = []
dhat-heap = ["dep:dhat"]
mimalloc = ["dep:mimalloc"]

[profile.release]
opt-level = 3
debug = 1
codegen-units = 16
lto = false

[workspace]
