#!/usr/bin/env bash
set -e

PORT=${1:-8080}
HOST=${2:-"127.0.0.1"}

echo "=== Building ReDash Web Server & Client ==="
cargo build --bin redash-server

echo "=== Starting ReDash Web Gateway on http://${HOST}:${PORT} ==="
if command -v open >/dev/null 2>&1; then
    (sleep 1 && open "http://${HOST}:${PORT}") &
fi

exec cargo run --bin redash-server -- --host "${HOST}" --port "${PORT}"
