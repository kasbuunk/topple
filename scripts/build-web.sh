#!/bin/sh
# Build the wasm module and assemble the static web bundle in web/.
# Requires: rustup target add wasm32-unknown-unknown
set -e
cd "$(dirname "$0")/.."
cargo build --release -p topple-web --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/topple_web.wasm web/
echo "Done. Serve web/ from any static host, e.g.:"
echo "  python3 -m http.server -d web 8080"
