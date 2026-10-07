#!/usr/bin/env bash
# Install Pergament from the release zip: the binaries, the icons and a
# desktop entry for the current user. Shipped as install.sh inside the zip.
#
#   ./install.sh                 # install below ~/.local
#   ./install.sh --prefix DIR    # install below DIR (for example /usr/local, as root)
#   ./install.sh --dry-run       # print what would be done
#   ./install.sh --uninstall     # remove what this script installed
#
# The app needs WebKitGTK 4.1, GTK 3, libsoup 3, OpenSSL and SQLite installed
# through your package manager. Your library in ~/.local/share/pergament is
# never touched.
set -euo pipefail

app_id=io.github.v3rm1n.pergament

prefix=${HOME}/.local
dry=0
uninstall=0

die() { echo "install: $*" >&2; exit 1; }
say() { echo "install: $*"; }
run() {
    if ((dry)); then echo "+ $*"; else "$@"; fi
}

while (($#)); do
    case $1 in
        --prefix) prefix=${2:?--prefix needs a directory}; shift ;;
        --dry-run) dry=1 ;;
        --uninstall) uninstall=1 ;;
        -h | --help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown option $1 (see --help)" ;;
    esac
    shift
done

[[ $(uname -s) == Linux ]] || die "Pergament runs on Linux only"
prefix=${prefix%/}
[[ -n $prefix ]] || die "--prefix must not be /"

refresh_caches() {
    command -v update-desktop-database >/dev/null && run update-desktop-database -q "$prefix/share/applications" || true
    command -v gtk-update-icon-cache >/dev/null && run gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true
}

if ((uninstall)); then
    for f in \
        "$prefix/bin/pergament" \
        "$prefix/bin/pergament-app" \
        "$prefix/share/applications/$app_id.desktop" \
        "$prefix/share/icons/hicolor/scalable/apps/$app_id.svg"; do
        [[ -e $f ]] && run rm -f "$f"
    done
    for size in 32x32 64x64 128x128 256x256 512x512; do
        f=$prefix/share/icons/hicolor/$size/apps/$app_id.png
        [[ -e $f ]] && run rm -f "$f"
    done
    refresh_caches
    say "removed. Your library in ~/.local/share/pergament is untouched."
    exit 0
fi

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
for f in bin/pergament bin/pergament-app share/icons/icon.svg; do
    [[ -f $here/$f ]] || die "$here/$f is missing; run this script from the unpacked zip"
done

if command -v ldd >/dev/null; then
    missing=$(ldd "$here/bin/pergament-app" 2>/dev/null | awk '/not found/ {print $1}' | sort -u | tr '\n' ' ')
    if [[ -n $missing ]]; then
        {
            echo "install: missing libraries: $missing"
            echo "  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0 libsoup-3.0-0 libssl3 libsqlite3-0"
            echo "  Fedora:        sudo dnf install webkit2gtk4.1 gtk3 libsoup3 openssl-libs sqlite-libs"
            echo "  Arch:          sudo pacman -S webkit2gtk-4.1 gtk3 libsoup3 openssl sqlite"
        } >&2
        ((dry)) || exit 1
    fi
fi

say "installing below $prefix"
run install -Dm755 "$here/bin/pergament" "$prefix/bin/pergament"
run install -Dm755 "$here/bin/pergament-app" "$prefix/bin/pergament-app"
for size in 32 64 128 256 512; do
    run install -Dm644 "$here/share/icons/${size}x${size}.png" \
        "$prefix/share/icons/hicolor/${size}x${size}/apps/$app_id.png"
done
run install -Dm644 "$here/share/icons/icon.svg" "$prefix/share/icons/hicolor/scalable/apps/$app_id.svg"

desktop=$prefix/share/applications/$app_id.desktop
if ((dry)); then
    echo "+ write $desktop"
else
    mkdir -p "$(dirname "$desktop")"
    cat >"$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Pergament
GenericName=Publication Reader
Comment=Pergament, an unofficial Linux reader for .jwpub publications
Exec=$prefix/bin/pergament-app
Icon=$app_id
Terminal=false
Categories=Office;Viewer;
Keywords=jwpub;Bible;reader;
StartupWMClass=pergament-app
DESKTOP
fi
refresh_caches

case :$PATH: in
    *:"$prefix/bin":*) ;;
    *) say "add $prefix/bin to your PATH to run pergament from a terminal" ;;
esac
say "done: start Pergament from your application menu or run pergament-app"
