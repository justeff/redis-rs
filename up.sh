#!/bin/sh
# Start (or restart) the benchmark environment.
#   MODE=podman (default): Redis container + one long-lived client container; runs use `podman exec`.
#   MODE=native (Linux):   redis-server on the host (port REDIS_PORT, default 6390); runs start the binary directly.
# Env: REDIS_CPU (core for redis-server, default 10).
set -e
cd "$(dirname "$0")"
MODE=${MODE:-podman}
REDIS_CPU=${REDIS_CPU:-10}
REDIS_PORT=${REDIS_PORT:-6390}
./down.sh >/dev/null 2>&1 || true
case $MODE in
  podman)
    podman run -d --name rrb-redis docker.io/library/redis:8-alpine \
      redis-server --save '' --appendonly no --server-cpulist "$REDIS_CPU" >/dev/null
    podman run -d --name rrb-client --network container:rrb-redis --security-opt label=disable \
      -v "$PWD":/work -w /work docker.io/library/rust:latest sleep infinity >/dev/null
    echo "podman: redis on core $REDIS_CPU, client container rrb-client ready"
    ;;
  native)
    [ "$(uname -s)" = Linux ] || { echo "MODE=native needs Linux (taskset, no VM)" >&2; exit 1; }
    command -v redis-server >/dev/null || { echo "redis-server not found in PATH" >&2; exit 1; }
    taskset -c "$REDIS_CPU" redis-server --port "$REDIS_PORT" --bind 127.0.0.1 --save '' --appendonly no \
      --daemonize yes --pidfile "$PWD/redis-native.pid" --logfile "$PWD/redis-native.log"
    sleep 0.5
    echo "native: $(redis-server --version | cut -d' ' -f1-3) on core $REDIS_CPU, port $REDIS_PORT"
    ;;
  *) echo "unknown MODE=$MODE" >&2; exit 1 ;;
esac
