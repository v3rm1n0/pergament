#!/usr/bin/env bash
# Cut a release without pushing to main. Two steps:
#
#   scripts/release.sh X.Y.Z          set version X.Y.Z everywhere on a branch
#                                     release/vX.Y.Z and open a pull request
#   scripts/release.sh --tag X.Y.Z    after that pull request is merged: tag main
#                                     and push the tag
#
# Pushing the tag runs .github/workflows/release.yml, which builds the binaries
# and publishes a GitHub release with a zip (binaries, icons, installer), its
# .sha256 file and the source code. The release notes are the ones GitHub
# generates from the merged pull requests (.github/release.yml), with the install
# instructions in front. Run inside `nix develop`; the first step needs `gh`.
set -euo pipefail
die() { echo "release: $*" >&2; exit 1; }

mode=prepare
if [[ ${1:-} == --tag ]]; then
    mode=tag
    shift
fi
version=${1:-}
[[ $version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    die "usage: scripts/release.sh [--tag] X.Y.Z (semantic version, no prefix or suffix)"
tag=v$version
cd "$(git rev-parse --show-toplevel)"

[[ $(git branch --show-current) == main ]] || die "releases are made from main"
[[ -z $(git status --porcelain) ]] || die "the working tree is not clean"
git fetch --quiet --tags origin
[[ $(git rev-parse HEAD) == $(git rev-parse origin/main) ]] || die "main and origin/main differ (pull first)"
! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "$tag exists already"

if [[ $mode == tag ]]; then
    scripts/versions.sh "$version" >/dev/null || die "main does not carry version $version; merge the release pull request first"
    [[ $(git log -1 --format=%s) == "chore: release $tag" ]] ||
        die "the newest commit on main is not \"chore: release $tag\"; merge the release pull request first"
    git tag -a "$tag" -m "Pergament $version"
    git push origin "$tag"
    echo "release: pushed $tag; the release workflow publishes it"
    exit 0
fi

command -v gh >/dev/null || die "gh is needed to open the pull request"
branch=release/$tag
! git rev-parse -q --verify "refs/heads/$branch" >/dev/null || die "branch $branch exists already"
last=$(git tag --list 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n1 || true)
if [[ -n $last ]]; then
    newest=$(printf '%s\n%s\n' "${last#v}" "$version" | sort -V | tail -n1)
    [[ $newest == "$version" && ${last#v} != "$version" ]] || die "$version is not above $last"
fi

echo "release: running checks"
cargo fmt --all --check
cargo clippy --workspace --all-targets -q -- -D warnings
cargo test --workspace -q
bun run lint
bun run test --run

echo "release: setting version $version on $branch"
git switch -q -c "$branch"
sed -i "0,/^version = \".*\"$/s//version = \"$version\"/" Cargo.toml src-tauri/Cargo.toml
sed -i "0,/^  \"version\": \".*\",$/s//  \"version\": \"$version\",/" package.json src-tauri/tauri.conf.json
cargo update --workspace --offline -q
scripts/versions.sh "$version" >/dev/null

git add Cargo.toml src-tauri/Cargo.toml package.json src-tauri/tauri.conf.json Cargo.lock
git commit -q -m "chore: release $tag"
git push -q -u origin "$branch"
# The release pull request itself does not belong in the generated notes (.github/release.yml).
gh label create skip-changelog --description "Left out of the generated release notes" --color ededed 2>/dev/null || true
gh pr create --base main --head "$branch" --title "chore: release $tag" --label skip-changelog \
    --body "Sets the version to $version. After merging, run \`scripts/release.sh --tag $version\` on main."
git switch -q main
echo "release: opened the pull request; after merging it run: scripts/release.sh --tag $version"
