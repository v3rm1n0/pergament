#!/usr/bin/env bash
# Cross-compiles the Rust core (ios/core) for aarch64-apple-ios and copies the
# static library to ios/lib, where Package.swift links it from. Run before
# `xtool dev`. Enters the flake's dev shell itself when started outside it.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
sdk="${PERGAMENT_IOS_SDK:-$HOME/.config/swiftpm/swift-sdks/darwin.artifactbundle/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS27.2.sdk}"
toolchain="${PERGAMENT_RUST_TOOLCHAIN:-1.98.1}"
clang="${PERGAMENT_IOS_CLANG:-/run/current-system/sw/bin/clang}"

if [[ -z "${IN_NIX_SHELL:-}" ]]; then
  exec nix develop "$root" --command "$root/ios/build-core.sh" "$@"
fi

target=aarch64-apple-ios
lib="$root/ios/lib"

# The nix rustc has no iOS std, so cargo runs the rustup toolchain's rustc.
RUSTC="$(rustup which --toolchain "$toolchain" rustc)"
export RUSTC
export SDKROOT="$sdk"
export CC_aarch64_apple_ios="$clang"
export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$clang"

(cd "$root" && cargo build -p pergament-ios --target "$target" --release)
mkdir -p "$lib"
cp "${CARGO_TARGET_DIR:-$root/target}/$target/release/libpergament_ios.a" "$lib/"
echo "Copied libpergament_ios.a to ios/lib"
