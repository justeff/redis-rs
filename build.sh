#!/bin/sh
# Usage: ./build.sh <variant>   (variant = directory under wt/)
# Builds rrbench plus its allocation-counting, dhat and mimalloc flavours.
#   MODE=podman (default): in the rust container, into builds/<variant>/ (Linux binaries)
#   MODE=native:           with the host's cargo, into builds-native/<variant>/
set -e
V=$1
cd "$(dirname "$0")"
FLAVOURS="rrbench: rrbench-count:count rrbench-dhat:dhat-heap rrbench-mi:mimalloc"
case ${MODE:-podman} in
  podman)
    OUT=builds/$V; mkdir -p "$OUT/src"
    sed "s#REDIS_PATH#/work/wt/$V/redis#" harness/Cargo.toml.tpl > "$OUT/Cargo.toml"
    cp harness/src/main.rs "$OUT/src/main.rs"
    podman run --rm --security-opt label=disable \
      -v "$PWD":/work -v rrbench-cargo:/usr/local/cargo/registry -v "rrbench-target-$V":/target \
      -w "/work/$OUT" docker.io/library/rust:latest sh -c "
        set -e
        for f in $FLAVOURS; do
          name=\${f%%:*}; feat=\${f#*:}
          CARGO_TARGET_DIR=/target/\$name cargo build --release -q \${feat:+--features \$feat} 2>&1 | grep -v '^warning' | tail -20
          cp /target/\$name/release/rrbench /work/$OUT/\$name
        done"
    ;;
  native)
    OUT=builds-native/$V; mkdir -p "$OUT/src"
    sed "s#REDIS_PATH#$PWD/wt/$V/redis#" harness/Cargo.toml.tpl > "$OUT/Cargo.toml"
    cp harness/src/main.rs "$OUT/src/main.rs"
    for f in $FLAVOURS; do
      name=${f%%:*}; feat=${f#*:}
      CARGO_TARGET_DIR="$PWD/target-native/$V/$name" cargo build --release -q --manifest-path "$OUT/Cargo.toml" ${feat:+--features $feat} 2>&1 | grep -v '^warning' | tail -20
      cp "target-native/$V/$name/release/rrbench" "$OUT/$name"
    done
    ;;
esac
ls -la "$OUT"/rrbench*
