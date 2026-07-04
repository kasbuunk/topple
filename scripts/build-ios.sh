#!/bin/sh
# Build the Rust static libraries for iOS and (on macOS) generate the Xcode
# project. Run this before opening ios/Topple.xcodeproj.
#
# Requires: rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
# On macOS additionally: brew install xcodegen
set -e
cd "$(dirname "$0")/.."

# Device slice.
cargo build --release -p topple-ios --target aarch64-apple-ios

# Simulator slices, merged into one universal library (Xcode picks the
# search path per SDK, so the two simulator archs must share one file).
cargo build --release -p topple-ios --target aarch64-apple-ios-sim
cargo build --release -p topple-ios --target x86_64-apple-ios
mkdir -p target/universal-ios-sim/release
if command -v lipo >/dev/null 2>&1; then
    lipo -create \
        target/aarch64-apple-ios-sim/release/libtopple_ios.a \
        target/x86_64-apple-ios/release/libtopple_ios.a \
        -output target/universal-ios-sim/release/libtopple_ios.a
else
    echo "note: lipo not found (not macOS?) — copying the arm64 sim slice only"
    cp target/aarch64-apple-ios-sim/release/libtopple_ios.a \
        target/universal-ios-sim/release/libtopple_ios.a
fi

if command -v xcodegen >/dev/null 2>&1; then
    (cd ios && xcodegen generate)
    echo "Done. Open ios/Topple.xcodeproj"
else
    echo "Done. On macOS: brew install xcodegen && (cd ios && xcodegen generate)"
fi
