#!/usr/bin/env bash
# Release notes for tag vX.Y.Z in Markdown: the commits since the previous
# release tag, grouped by their conventional commit type.
set -euo pipefail
tag=${1:?usage: scripts/release-notes.sh vX.Y.Z}
cd "$(git rev-parse --show-toplevel)"

previous=$(git tag --list 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | grep -vx "$tag" | head -n1 || true)
range=${previous:+$previous..}$tag

# owner/name of the GitHub repository: set in Actions, else read from the origin remote.
repo=${GITHUB_REPOSITORY:-}
if [[ -z $repo ]]; then
    repo=$(git remote get-url origin 2>/dev/null | sed -nE 's#^(git@github.com:|https://github.com/)([^/]+/[^/]+)$#\2#p' | sed -E 's/\.git$//' || true)
fi

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

echo 'Download the zip for Linux, unpack it and run `./install.sh`. It installs the binaries and a desktop entry below `~/.local`. Check the download against the `.sha256` file with `sha256sum -c`. The README also describes building from source.'
echo
section Features feat
section Fixes fix
section Documentation docs
section Other 'refactor|perf|style|test|build|ci|chore'
if [[ -n $previous && -n $repo ]]; then
    echo "All changes: [$previous...$tag](https://github.com/$repo/compare/$previous...$tag)"
elif [[ -n $previous ]]; then
    echo "All changes: $previous...$tag"
fi
