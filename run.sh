#!/bin/zsh
# run.sh <variant> <extra args...>  -> one JSON line
cd "$(dirname "$0")"
V=$1; shift
BIN=${BIN:-rrbench}
podman run --rm --network container:rrb-redis --security-opt label=disable -v "$PWD":/work docker.io/library/rust:latest \
  taskset -c ${CLIENT_CPUS:-4-7} /work/builds/$V/$BIN --label $V "$@" 2>&1 | tail -1
