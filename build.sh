#!/bin/zsh
# Usage: ./build.sh <variant>   (variant = directory under wt/)
# Builds rrbench (timing) and rrbench-count (allocation counting) for Linux inside the rust container.
set -e
V=$1
cd "$(dirname "$0")"
mkdir -p builds/$V/src
sed "s#REDIS_PATH#/work/wt/$V/redis#" harness/Cargo.toml.tpl > builds/$V/Cargo.toml
cp harness/src/main.rs builds/$V/src/main.rs
podman run --rm --security-opt label=disable \
  -v "$PWD":/work -v rrbench-cargo:/usr/local/cargo/registry -v rrbench-target-$V:/target \
  -e CARGO_TARGET_DIR=/target -w /work/builds/$V docker.io/library/rust:latest \
  sh -c 'cargo build --release -q 2>&1 | grep -v "^warning" | tail -20; cp /target/release/rrbench /work/builds/'$V'/rrbench; \
         CARGO_TARGET_DIR=/target/count cargo build --release -q --features count 2>&1 | grep -v "^warning" | tail -20; cp /target/count/release/rrbench /work/builds/'$V'/rrbench-count; \
         CARGO_TARGET_DIR=/target/dhat cargo build --release -q --features dhat-heap 2>&1 | grep -v "^warning" | tail -20; cp /target/dhat/release/rrbench /work/builds/'$V'/rrbench-dhat; \
         CARGO_TARGET_DIR=/target/mi cargo build --release -q --features mimalloc 2>&1 | grep -v "^warning" | tail -20; cp /target/mi/release/rrbench /work/builds/'$V'/rrbench-mi'
ls -la builds/$V/rrbench builds/$V/rrbench-count
