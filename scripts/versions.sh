#!/usr/bin/env bash
# Print the version of every manifest, one "file version" per line.
# With an argument X.Y.Z, fail unless all of them equal it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

versions() {
    for f in Cargo.toml src-tauri/Cargo.toml; do
        echo "$f $(sed -n 's/^version = "\(.*\)"$/\1/p' "$f" | head -n1)"
    done
    for f in package.json src-tauri/tauri.conf.json; do
        echo "$f $(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$f" | head -n1)"
    done
}

versions
if [[ $# -gt 0 ]]; then
    bad=$(versions | awk -v want="$1" '$2 != want')
    if [[ -n $bad ]]; then
        echo "version mismatch, expected $1:" >&2
        echo "$bad" >&2
        exit 1
    fi
fi
