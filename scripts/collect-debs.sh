#!/usr/bin/env bash
# Download the linux x86_64 .deb of the last COUNT (default 3) published
# releases that have one into OUT_DIR, newest first, for scripts/package-apt.sh.
# Drafts and prereleases are ignored; releases without a deb are skipped.
# Needs gh with GH_TOKEN (or a login). The repository comes from
# GITHUB_REPOSITORY, defaulting to pergament-project/pergament, because a clone with
# several remotes leaves gh unable to pick one.
set -euo pipefail
die() { echo "collect-debs: $*" >&2; exit 1; }

out_dir=${1:?usage: scripts/collect-debs.sh OUT_DIR [COUNT]}
count=${2:-3}
[[ $count =~ ^[1-9][0-9]*$ ]] || die "COUNT must be a positive integer, got '$count'"
repo=${GITHUB_REPOSITORY:-pergament-project/pergament}
out_dir=$(realpath -m "$out_dir")
cd "$(git rev-parse --show-toplevel)"
command -v gh >/dev/null || die "gh is required"

mkdir -p "$out_dir"
tags=$(gh release list --repo "$repo" --exclude-drafts --exclude-pre-releases \
    --limit 100 --json tagName --jq '.[].tagName')

taken=0
for tag in $tags; do
    ((taken < count)) || break
    # Captured first so a failed lookup aborts instead of reading as "no deb".
    names=$(gh release view "$tag" --repo "$repo" --json assets --jq '.assets[].name')
    if ! grep -Eq '^Pergament-.+-linux-x86_64\.deb$' <<<"$names"; then
        echo "collect-debs: skip $tag (no deb)"
        continue
    fi
    gh release download "$tag" --repo "$repo" --dir "$out_dir" \
        --pattern 'Pergament-*-linux-x86_64.deb'
    echo "collect-debs: take $tag"
    taken=$((taken + 1))
done
((taken > 0)) || die "no deb found in the published releases of $repo"
