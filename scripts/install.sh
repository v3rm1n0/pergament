#!/usr/bin/env bash
# Build Pergament from source and install it for the current user.
#
#   scripts/install.sh                 # from a checkout
#   curl -fsSL https://raw.githubusercontent.com/v3rm1n0/pergament/main/scripts/install.sh | bash
#
# With Nix it installs the flake package into your profile. Without Nix it
# builds with cargo and bun and copies the files below ~/.local (or --prefix).
# Nothing is downloaded except the source code and the build dependencies.
#
#   --prefix DIR   install below DIR instead of ~/.local (without Nix only)
#   --no-nix       build with cargo even if Nix is installed
#   --dry-run      print what would be done
#   --uninstall    remove what this script installed
set -euo pipefail

repo=https://github.com/v3rm1n0/pergament.git
flake=github:v3rm1n0/pergament
app_id=io.github.v3rm1n.pergament

prefix=${HOME}/.local
use_nix=auto
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
        --no-nix) use_nix=no ;;
        --dry-run) dry=1 ;;
        --uninstall) uninstall=1 ;;
        -h | --help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown option $1 (see --help)" ;;
    esac
    shift
done

[[ $(uname -s) == Linux ]] || die "Pergament runs on Linux only"

have_nix() {
    command -v nix >/dev/null && nix --extra-experimental-features 'nix-command flakes' --version >/dev/null 2>&1
}
nix_cmd() { nix --extra-experimental-features 'nix-command flakes' "$@"; }

want_nix=0
if [[ $use_nix != no ]] && have_nix; then want_nix=1; fi

if ((uninstall)); then
    if ((want_nix)) && nix_cmd profile list 2>/dev/null | grep -q pergament; then
        say "removing the Nix profile package"
        run nix_cmd profile remove pergament
    fi
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
    say "removed. Your library in ~/.local/share/pergament is untouched."
    exit 0
fi

# Where the source is: this checkout, or a fresh clone of the newest release.
src=
cleanup() { [[ -n ${tmp:-} ]] && rm -rf "$tmp"; }
trap cleanup EXIT
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)
if [[ -n $script_dir && -f $script_dir/../Cargo.toml && -f $script_dir/../flake.nix ]]; then
    src=$(cd "$script_dir/.." && pwd)
fi

if ((want_nix)); then
    if [[ -n $src ]]; then
        say "building with Nix from $src"
        run nix_cmd profile install "$src#pergament"
    else
        say "building with Nix from $flake"
        run nix_cmd profile install "$flake#pergament"
    fi
    say "done: run pergament-app (or pergament for the command line tool)"
    exit 0
fi

missing=()
for tool in git cargo rustc bun pkg-config cc; do
    command -v "$tool" >/dev/null || missing+=("$tool")
done
if command -v pkg-config >/dev/null; then
    for lib in webkit2gtk-4.1 gtk+-3.0 libsoup-3.0 openssl sqlite3; do
        pkg-config --exists "$lib" || missing+=("$lib (development files)")
    done
fi
if ((${#missing[@]})); then
    {
        echo "install: missing: ${missing[*]}"
        echo "  Debian/Ubuntu: sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev libsqlite3-dev git curl"
        echo "  Fedora:        sudo dnf install gcc pkgconf-pkg-config webkit2gtk4.1-devel gtk3-devel libsoup3-devel openssl-devel sqlite-devel git"
        echo "  Arch:          sudo pacman -S base-devel webkit2gtk-4.1 gtk3 libsoup3 openssl sqlite git"
        echo "  Rust:          https://rustup.rs      Bun: https://bun.sh"
        echo "  Or install Nix (https://nixos.org/download) and run this script again."
    } >&2
    ((dry)) || exit 1
fi

if [[ -z $src ]]; then
    tmp=$(mktemp -d)
    say "fetching the newest release"
    tag=$(git ls-remote --tags --sort=-v:refname "$repo" 'v*' | grep -v '\^{}' | head -n1 | sed 's#.*refs/tags/##')
    [[ -n $tag ]] || die "no release tag found"
    run git clone --quiet --depth 1 --branch "$tag" "$repo" "$tmp/pergament"
    src=$tmp/pergament
fi

say "building (this takes a few minutes)"
if ((dry)); then
    echo "+ cd $src && bun install --frozen-lockfile && bun run build"
    echo "+ cd $src && cargo build --release -p pergament -p pergament-app --features pergament-app/custom-protocol"
else
    (
        cd "$src"
        bun install --frozen-lockfile
        bun run build
        cargo build --release -p pergament -p pergament-app --features pergament-app/custom-protocol
    )
fi

say "installing below $prefix"
run install -Dm755 "$src/target/release/pergament" "$prefix/bin/pergament"
run install -Dm755 "$src/target/release/pergament-app" "$prefix/bin/pergament-app"
for size in 32 64 128 256; do
    run install -Dm644 "$src/src-tauri/icons/${size}x${size}.png" \
        "$prefix/share/icons/hicolor/${size}x${size}/apps/$app_id.png"
done
run install -Dm644 "$src/src-tauri/icons/icon.png" "$prefix/share/icons/hicolor/512x512/apps/$app_id.png"
run install -Dm644 "$src/src-tauri/icons/icon.svg" "$prefix/share/icons/hicolor/scalable/apps/$app_id.svg"

desktop=$prefix/share/applications/$app_id.desktop
if ((dry)); then
    echo "+ write $desktop"
else
    mkdir -p "$(dirname "$desktop")"
    cat >"$desktop" <<EOF
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
EOF
fi

case :$PATH: in
    *:"$prefix/bin":*) ;;
    *) say "add $prefix/bin to your PATH to run pergament from a terminal" ;;
esac
say "done: start Pergament from your application menu or run pergament-app"
