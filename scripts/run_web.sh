#!/usr/bin/env bash
set -e

: "${REDASH_GATEWAY_TOKEN:?Set a random management token (at least 32 characters) before starting the Gateway}"

PORT=${1:-8080}
HOST=${2:-"127.0.0.1"}

export PATH="$HOME/.cargo/bin:$PATH"

echo "=== Building ReDash GPUI Web Client (WebAssembly) ==="
bash "$(dirname "$0")/build_web.sh"

echo "=== Building ReDash Web Gateway Server ==="
cargo build --bin redash-server

echo "=== Starting ReDash Web Gateway on http://${HOST}:${PORT} ==="
if command -v open >/dev/null 2>&1; then
    (sleep 1 && open "http://${HOST}:${PORT}") &
fi

exec cargo run --bin redash-server -- --host "${HOST}" --port "${PORT}"
