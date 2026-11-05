# rrbench

End-to-end client-cost benchmark for the redis-rs `MultiplexedConnection`, used to measure
`perf/driver-owned-deadlines` and `perf/caller-side-decoding`. This branch is tooling only and
is not meant for a pull request.

One connection is shared by T tasks in a closed loop with a fixed operation count. The harness
reports throughput and the client process CPU (user+sys from `getrusage`) per operation. Results
are converted into the types an application would use (`Option<String>`, `Vec<String>`,
`HashMap<String, String>`).

## Modes

| `MODE` | Redis | Client | Use for |
|---|---|---|---|
| `podman` (default) | container `rrb-redis` | long-lived container `rrb-client`, one `podman exec` per run | macOS, or Linux without a Rust toolchain |
| `native` (Linux only) | host `redis-server` on port `REDIS_PORT` (default 6390) | binary started directly on the host | final numbers: no container or VM in the path |

Set `MODE` the same way for every script. Podman builds go to `builds/`, native builds to
`builds-native/`, because container binaries and the host's glibc need not match.

## Requirements

- `podman` mode: `podman`, `git`, `zsh` (for `matrix.sh`), `python3`. Everything builds and runs
  in `docker.io/library/rust:latest`; Redis runs from `docker.io/library/redis:8-alpine`.
- `native` mode: additionally `cargo` (stable), `redis-server`, `taskset` (util-linux).

## Setup

```sh
export MODE=native REDIS_CPU=1 CLIENT_CPUS=4-7   # or MODE=podman
./setup.sh                        # clone the fork, create wt/{base,drv,cd,fastpath,combined}, run up.sh
for v in base drv cd fastpath combined; do ./build.sh $v; done
./up.sh                           # (re)start Redis, and in podman mode the client container
./down.sh                         # stop everything when done
```

Pick cores that are otherwise idle: Redis on `REDIS_CPU`, the client on `CLIENT_CPUS` (four
cores of the same type, used by 4 Tokio worker threads). Redis and the client talk over loopback.

| Variant | Contents |
|---|---|
| `base` | upstream `main` the PRs are based on |
| `drv` | `perf/driver-owned-deadlines` |
| `cd` | `perf/caller-side-decoding` (without #2184, regresses small replies) |
| `fastpath` | `main` + upstream PR #2184 |
| `combined` | `main` + #2184 + both PRs |

## Measure

```sh
# MODE, CLIENT_CPUS as above
WORKLOADS="get set get1k mget100 hgetall pipe100" TASKS="16 128" \
  ./matrix.sh results/final.jsonl 5 base drv fastpath cd combined
python3 summarize.py results/final.jsonl
```

`matrix.sh` rotates the variant order on every repetition to cancel drift. `summarize.py`
prints the median per cell and the change against `base`.

More runs:

- Latency-bound: `TASKS="1"`.
- RESP3: `EXTRA="--resp3"`.
- Allocations per op: `BIN=rrbench-count ./run.sh <variant> --workload mget100 --tasks 16 --ops 30000`.
- Boxed-future cost (2.0 idea): workload `getdirect` against `get`.
- Integration tests (nextest, RESP2/RESP3): `podman build -t rrbench-test -f testimg.Containerfile .`,
  then `./itest.sh <variant> RESP2`.
- Microbenchmarks for the 2.0 ideas: `cd micro && cargo run --release` (native, no server).
