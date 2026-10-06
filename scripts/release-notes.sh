#!/usr/bin/env bash
# Release notes for tag vX.Y.Z in Markdown: the commits since the previous
# release tag, grouped by their conventional commit type.
set -euo pipefail
tag=${1:?usage: scripts/release-notes.sh vX.Y.Z}
cd "$(git rev-parse --show-toplevel)"

previous=$(git tag --list 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | grep -vx "$tag" | head -n1 || true)
range=${previous:+$previous..}$tag

section() {
    local title=$1 pattern=$2 lines
    lines=$(git log --no-merges --format='%s' "$range" |
        grep -E "^($pattern)(\(.+\))?!?: " |
        grep -vE '^chore: release v' |
        sed -E 's/^[a-z]+(\(.+\))?!?: /- /' || true)
    if [[ -n $lines ]]; then
        printf '## %s\n\n%s\n\n' "$title" "$lines"
    fi
}

echo "Source code only. There are no official builds; see the README for how to build Pergament."
echo
section Features feat
section Fixes fix
section Documentation docs
section Other 'refactor|perf|style|test|build|ci|chore'
if [[ -n $previous ]]; then
    echo "All changes: $previous...$tag"
fi
