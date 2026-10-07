#!/usr/bin/env bash
# Pack the release build into target/package/pergament-X.Y.Z-linux-<arch>.zip with a
# .sha256 file next to it. Build first:
#   bun run build
#   cargo build --release -p pergament -p pergament-app --features pergament-app/custom-protocol
set -euo pipefail
die() { echo "package: $*" >&2; exit 1; }

version=${1:?usage: scripts/package.sh X.Y.Z}
cd "$(git rev-parse --show-toplevel)"
scripts/versions.sh "$version" >/dev/null

arch=$(uname -m)
name=pergament-$version-linux-$arch
out=target/package
stage=$out/$name

for f in target/release/pergament target/release/pergament-app; do
    [[ -x $f ]] || die "$f is missing; build the release first"
done

rm -rf "$stage" "$out/$name.zip" "$out/$name.zip.sha256"
mkdir -p "$stage/bin" "$stage/share/icons"
install -m755 target/release/pergament target/release/pergament-app "$stage/bin/"
for size in 32 64 128 256; do
    install -m644 "src-tauri/icons/${size}x${size}.png" "$stage/share/icons/"
done
install -m644 src-tauri/icons/icon.png "$stage/share/icons/512x512.png"
install -m644 src-tauri/icons/icon.svg "$stage/share/icons/"
install -m755 scripts/bundle-install.sh "$stage/install.sh"
install -m644 LICENSE README.md "$stage/"

(
    cd "$out"
    zip -qr -X "$name.zip" "$name"
    sha256sum "$name.zip" >"$name.zip.sha256"
)
rm -rf "$stage"
echo "package: $out/$name.zip"
cat "$out/$name.zip.sha256"
