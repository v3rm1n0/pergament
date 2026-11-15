#!/usr/bin/env bash
# Copy the Tauri bundles into target/package as Pergament-X.Y.Z-linux-x86_64.deb,
# Pergament-X.Y.Z-linux-x86_64.rpm and Pergament-X.Y.Z-x86_64.AppImage (no
# "linux", as the AppImage catalog asks), and write the AppImage's .zsync file for AppImageUpdate. Needs zsyncmake.
# Build and bundle first, with the update information embedded:
#   bun install --frozen-lockfile
#   cargo build --release -p pergament
#   cp target/release/pergament target/release/pergament-x86_64-unknown-linux-gnu
#   LDAI_UPDATE_INFORMATION='gh-releases-zsync|pergament-project|pergament|latest|Pergament-*x86_64.AppImage.zsync' \
#   bun run tauri build --bundles deb,rpm,appimage \
#       --config '{"bundle":{"active":true,"externalBin":["../target/release/pergament"]}}'
set -euo pipefail
die() { echo "package-bundles: $*" >&2; exit 1; }

version=${1:?usage: scripts/package-bundles.sh X.Y.Z}
cd "$(git rev-parse --show-toplevel)"
scripts/versions.sh "$version" >/dev/null

arch=x86_64
out=target/package
mkdir -p "$out"

shopt -s nullglob
deb=(target/release/bundle/deb/*.deb)
rpm=(target/release/bundle/rpm/*.rpm)
appimage=(target/release/bundle/appimage/*.AppImage)
shopt -u nullglob
((${#deb[@]} == 1)) || die "expected one deb bundle, found ${#deb[@]}"
((${#rpm[@]} == 1)) || die "expected one rpm bundle, found ${#rpm[@]}"
((${#appimage[@]} == 1)) || die "expected one appimage bundle, found ${#appimage[@]}"

install -m644 "$deb" "$out/Pergament-$version-linux-$arch.deb"
install -m644 "$rpm" "$out/Pergament-$version-linux-$arch.rpm"
install -m644 "$appimage" "$out/Pergament-$version-$arch.AppImage"

files=(
    "Pergament-$version-linux-$arch.deb"
    "Pergament-$version-linux-$arch.rpm"
    "Pergament-$version-$arch.AppImage"
)
(
    cd "$out"
    # The bundler's own .zsync names the file it wrote, so it is made again for the new name.
    zsyncmake -u "${files[2]}" -o "${files[2]}.zsync" "${files[2]}"
)
for f in "${files[@]}" "${files[2]}.zsync"; do
    echo "package-bundles: $out/$f"
done
