#!/usr/bin/env bash
# Builds the frontend and copies dist/ into the iOS package as the `web` resource.
# Run before `xtool dev`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
web="$root/ios/Sources/Pergament/web"

(cd "$root" && bun run build)
rm -rf "$web"
cp -r "$root/dist" "$web"
echo "Copied dist/ to ios/Sources/Pergament/web"
