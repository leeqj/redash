#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v wasm-bindgen >/dev/null 2>&1; then
    echo 'Install wasm-bindgen-cli 0.2.128 before building the Web client.' >&2
    exit 1
fi
if [[ "$(wasm-bindgen --version)" != 'wasm-bindgen 0.2.128' ]]; then
    echo 'This Cargo.lock requires wasm-bindgen-cli 0.2.128.' >&2
    exit 1
fi
cargo build --locked --target wasm32-unknown-unknown -p redash-web --release
wasm-bindgen "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/redash_web.wasm" \
    --out-dir crates/redash-server/web/pkg --target web --no-typescript
web_revision="${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || printf unknown)}"
web_wasm=crates/redash-server/web/pkg/redash_web_bg.wasm
if command -v sha256sum >/dev/null 2>&1; then
    web_hash=$(sha256sum "$web_wasm" | cut -d ' ' -f 1)
else
    web_hash=$(shasum -a 256 "$web_wasm" | cut -d ' ' -f 1)
fi
printf '{"revision":"%s","wasm_sha256":"%s"}\n' "$web_revision" "$web_hash" > crates/redash-server/web/pkg/build.json
