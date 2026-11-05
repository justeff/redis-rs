#!/bin/sh
# One-time setup: clone the fork, create one worktree per variant, start a pinned Redis.
# Env: REPO (fork URL), REDIS_CPU (core for redis-server, default 10).
set -e
cd "$(dirname "$0")"
REPO=${REPO:-https://github.com/justeff/redis-rs}
REDIS_CPU=${REDIS_CPU:-10}
mkdir -p wt results prof builds
[ -d src ] || git clone -q "$REPO" src
git -C src fetch -q origin
add() { [ -d "wt/$1" ] || git -C src worktree add -q --detach "../wt/$1" "$2"; echo "wt/$1 -> $(git -C "wt/$1" log -1 --format='%h %s')"; }
add base       origin/bench/combined~1~1~1    # upstream main the PRs are based on
add drv        origin/perf/driver-owned-deadlines
add cd         origin/perf/caller-side-decoding
add fastpath   origin/bench/combined~1~1      # main + upstream PR #2184
add combined   origin/bench/combined          # main + #2184 + both PRs
podman rm -f rrb-redis >/dev/null 2>&1 || true
podman run -d --name rrb-redis docker.io/library/redis:8-alpine \
  redis-server --save '' --appendonly no --server-cpulist "$REDIS_CPU" >/dev/null
echo "redis started on core $REDIS_CPU"
