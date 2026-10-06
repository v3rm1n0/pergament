#!/usr/bin/env bash
# Build the Android or iOS app from source.
#   scripts/mobile.sh android [tauri args]   e.g. --apk --debug, --aab
#   scripts/mobile.sh ios [tauri args]       e.g. --debug --target aarch64-sim
# Needs the SDKs described in the README (Android: ANDROID_HOME and NDK_HOME,
# a JDK; iOS: macOS with Xcode). The Rust targets are installed with rustup.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

platform=${1:-}
shift || true

case $platform in
android)
    : "${ANDROID_HOME:?set ANDROID_HOME to the Android SDK}"
    : "${NDK_HOME:?set NDK_HOME to the Android NDK}"
    rustup target add aarch64-linux-android armv7-linux-androideabi \
        i686-linux-android x86_64-linux-android
    pnpm install --frozen-lockfile
    pnpm tauri android build "$@"
    ;;
ios)
    [[ $(uname) == Darwin ]] || { echo "iOS apps can only be built on macOS" >&2; exit 1; }
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
    pnpm install --frozen-lockfile
    # The Xcode project is generated on a Mac and not kept in the repository.
    [[ -d src-tauri/gen/apple ]] || pnpm tauri ios init --ci
    pnpm tauri ios build "$@"
    ;;
*)
    echo "usage: $0 android|ios [tauri args]" >&2
    exit 2
    ;;
esac
