#!/bin/zsh
# itest.sh <variant> <RESP2|RESP3> -> async + cluster integration tests (nextest, profile tcp) in Linux
cd "$(dirname "$0")"
V=$1; P=$2
FEATURES=tokio-comp,smol-comp,connection-manager,cache-aio,cluster-async,streams,script,acl,bloom
podman run --rm --security-opt label=disable -v "$PWD":/work -v rrbench-cargo:/usr/local/cargo/registry -v rrbench-ttarget-$V:/target \
  -e CARGO_TARGET_DIR=/target -e REDISRS_SERVER_TYPE=tcp -e PROTOCOL=$P -w /work/wt/$V rrbench-test \
  sh -c "cargo nextest run --no-fail-fast -p redis --features $FEATURES --profile tcp --test-threads 8 -E 'binary(test_async) | binary(test_cache) | binary(test_cluster_async) | binary(test_script) | binary(test_streams) | binary(test_types) | kind(lib)' 2>&1 | grep -E '^\s+(FAIL|TIMEOUT|SIGSEGV|SIGABRT)|Summary|tests run' | sort -u | head -80"
