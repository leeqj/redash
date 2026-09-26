#!/usr/bin/env bash
set -e

PORT=${1:-8080}
HOST=${2:-"127.0.0.1"}

export PATH="$HOME/.cargo/bin:$PATH"

echo "=== Building ReDash GPUI Web Client (WebAssembly) ==="
cargo build --target wasm32-unknown-unknown -p redash-web --release

if command -v wasm-bindgen >/dev/null 2>&1; then
    wasm-bindgen target/wasm32-unknown-unknown/release/redash_web.wasm \
        --out-dir crates/redash-server/web/pkg \
        --target web \
        --no-typescript
else
    echo "Note: wasm-bindgen not in PATH, using pre-bundled artifacts in crates/redash-server/web/pkg"
fi

echo "=== Building ReDash Web Gateway Server ==="
cargo build --bin redash-server

echo "=== Starting ReDash Web Gateway on http://${HOST}:${PORT} ==="
if command -v open >/dev/null 2>&1; then
    (sleep 1 && open "http://${HOST}:${PORT}") &
fi

exec cargo run --bin redash-server -- --host "${HOST}" --port "${PORT}"
