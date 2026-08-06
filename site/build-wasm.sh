#!/usr/bin/env bash
# Build the demos for the web into site/wasm/.
# Usage: build-wasm.sh [webgpu|webgl2|assets|all]   (default: all)
#   webgpu/webgl2 build one backend (CI builds them as parallel jobs),
#   assets emits site/assets + the version stamp, all does everything.
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
    --example theme_gallery --example rpg_hud --example navigation \
    --example wpf_samples_gallery --example snippets_gallery \
    --target wasm32-unknown-unknown --profile wasm-release $features
  for app in breakout components_showcase theme_gallery rpg_hud navigation wpf_samples_gallery snippets_gallery; do
    local out="site/wasm/$backend"
    mkdir -p "$out"
    local name="$app"
    [ "$app" = "components_showcase" ] && name="showcase"
    [ "$app" = "theme_gallery" ] && name="themes"
    [ "$app" = "rpg_hud" ] && name="rpg"
    [ "$app" = "wpf_samples_gallery" ] && name="wpf"
    [ "$app" = "snippets_gallery" ] && name="snippets"
    wasm-bindgen --target web --no-typescript \
      --out-dir "$out" --out-name "$name" \
      "$TARGET_DIR/wasm32-unknown-unknown/wasm-release/examples/$app.wasm"
    if command -v wasm-opt >/dev/null; then
      wasm-opt -Oz --strip-debug --strip-producers -all \
        -o "$out/${name}_bg.wasm" "$out/${name}_bg.wasm"
    fi
  done
}

emit_assets() {
  # Assets the showcase fetches at runtime (bevy fetches relative to the page).
  mkdir -p site/assets/ui
  cp crates/bevy_pf/assets/ui/bench.png site/assets/ui/bench.png

  # Stamp the build so the loader cache-busts stale wasm after each deploy.
  STAMP="$(git rev-parse --short HEAD 2>/dev/null || echo dev)-$(date +%s)"
  echo "export const BUILD = '$STAMP';" > site/version.js
  echo "assets + version.js ready (build $STAMP)"
}

MODE="${1:-all}"
case "$MODE" in
  webgpu) build_backend webgpu "--features bevy/webgpu" ;;
  webgl2) build_backend webgl2 "" ;;
  assets) emit_assets ;;
  all)
    build_backend webgpu "--features bevy/webgpu"
    build_backend webgl2 ""
    emit_assets
    echo "site/wasm ready:"
    du -sh site/wasm/*
    ;;
  *) echo "usage: $0 [webgpu|webgl2|assets|all]" >&2; exit 1 ;;
esac
