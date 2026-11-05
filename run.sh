#!/bin/sh
# run.sh <variant> <extra args...>  -> one JSON line. Needs ./up.sh first.
# Env: MODE (podman|native), CLIENT_CPUS (default 4-7), BIN (rrbench|rrbench-count|rrbench-mi|rrbench-dhat).
cd "$(dirname "$0")"
V=$1; shift
BIN=${BIN:-rrbench}
CPUS=${CLIENT_CPUS:-4-7}
case ${MODE:-podman} in
  podman) podman exec rrb-client taskset -c "$CPUS" "builds/$V/$BIN" --label "$V" "$@" 2>&1 | tail -1 ;;
  native) BENCH_HOST=127.0.0.1:${REDIS_PORT:-6390} taskset -c "$CPUS" "./builds-native/$V/$BIN" --label "$V" "$@" 2>&1 | tail -1 ;;
esac
