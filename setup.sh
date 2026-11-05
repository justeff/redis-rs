#!/bin/sh
# One-time setup: clone the fork, create one worktree per variant, then start the environment (up.sh).
# Env: REPO (fork URL), MODE, REDIS_CPU, REDIS_PORT (see up.sh).
set -e
cd "$(dirname "$0")"
REPO=${REPO:-https://github.com/justeff/redis-rs}
mkdir -p wt results prof builds
[ -d src ] || git clone -q "$REPO" src
git -C src fetch -q origin
add() { [ -d "wt/$1" ] || git -C src worktree add -q --detach "../wt/$1" "$2"; echo "wt/$1 -> $(git -C "wt/$1" log -1 --format='%h %s')"; }
add base       origin/bench/combined~1~1~1    # upstream main the PRs are based on
add drv        origin/perf/driver-owned-deadlines
add cd         origin/perf/caller-side-decoding
add fastpath   origin/bench/combined~1~1      # main + upstream PR #2184
add combined   origin/bench/combined          # main + #2184 + both PRs
./up.sh
