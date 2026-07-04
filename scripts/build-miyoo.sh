#!/bin/sh
# Cross-compile for the Miyoo Mini Plus and assemble the OnionOS app folder.
# Requires: rustup target add armv7-unknown-linux-musleabihf
set -e
cd "$(dirname "$0")/.."
cargo build --release -p topple-miyoo --target armv7-unknown-linux-musleabihf
cp target/armv7-unknown-linux-musleabihf/release/topple-miyoo dist/miyoo/Topple/
echo "Done. Copy dist/miyoo/Topple/ to /mnt/SDCARD/App/Topple/ on the SD card."
