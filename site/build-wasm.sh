#!/usr/bin/env bash
# Build the two demos for the web, both backends, into site/wasm/.
# Requires: rustup target add wasm32-unknown-unknown
#           cargo install wasm-bindgen-cli --version 0.2.126 --locked
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTFLAGS='--cfg getrandom_backend="wasm_js"'
TARGET_DIR="${CARGO_TARGET_DIR:-target-wasm}"
export CARGO_TARGET_DIR="$TARGET_DIR"

build_backend() {
  local backend="$1" features="$2"
  echo "=== building $backend ==="
  # shellcheck disable=SC2086
  cargo build -p bevy_pf --example breakout --example components_showcase \
    --target wasm32-unknown-unknown --profile wasm-release $features
  for app in breakout components_showcase; do
    local out="site/wasm/$backend"
    mkdir -p "$out"
    local name="$app"
    [ "$app" = "components_showcase" ] && name="showcase"
    wasm-bindgen --target web --no-typescript \
      --out-dir "$out" --out-name "$name" \
      "$TARGET_DIR/wasm32-unknown-unknown/wasm-release/examples/$app.wasm"
    if command -v wasm-opt >/dev/null; then
      wasm-opt -Oz --strip-debug --strip-producers -all \
        -o "$out/${name}_bg.wasm" "$out/${name}_bg.wasm"
    fi
  done
}

build_backend webgpu "--features bevy/webgpu"
build_backend webgl2 ""

# Assets the showcase fetches at runtime (bevy fetches relative to the page).
mkdir -p site/assets/ui
cp crates/bevy_pf/assets/ui/bench.png site/assets/ui/bench.png

echo "site/wasm ready:"
du -sh site/wasm/*
