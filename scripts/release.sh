#!/usr/bin/env bash
# Cut a release: set version X.Y.Z everywhere, commit, tag vX.Y.Z and push.
# Pushing the tag runs .github/workflows/release.yml, which publishes a
# GitHub release with the source code only. Run inside `nix develop`.
set -euo pipefail
die() { echo "release: $*" >&2; exit 1; }

version=${1:-}
[[ $version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    die "usage: scripts/release.sh X.Y.Z (semantic version, no prefix or suffix)"
tag=v$version
cd "$(git rev-parse --show-toplevel)"

[[ $(git branch --show-current) == main ]] || die "releases are made from main"
[[ -z $(git status --porcelain) ]] || die "the working tree is not clean"
git fetch --quiet --tags origin
[[ $(git rev-parse HEAD) == $(git rev-parse origin/main) ]] || die "main and origin/main differ"
! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "$tag exists already"
last=$(git tag --list 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n1 || true)
if [[ -n $last ]]; then
    newest=$(printf '%s\n%s\n' "${last#v}" "$version" | sort -V | tail -n1)
    [[ $newest == "$version" && ${last#v} != "$version" ]] || die "$version is not above $last"
fi

echo "release: running checks"
cargo fmt --all --check
cargo clippy --workspace --all-targets -q -- -D warnings
cargo test --workspace -q
pnpm lint
pnpm test --run
nix build --no-link .#pergament

echo "release: setting version $version"
sed -i "0,/^version = \".*\"$/s//version = \"$version\"/" Cargo.toml src-tauri/Cargo.toml
sed -i "0,/^  \"version\": \".*\",$/s//  \"version\": \"$version\",/" package.json src-tauri/tauri.conf.json
cargo update --workspace --offline -q
scripts/versions.sh "$version" >/dev/null

git add Cargo.toml src-tauri/Cargo.toml package.json src-tauri/tauri.conf.json Cargo.lock
git commit -q -m "chore: release $tag"
git tag -a "$tag" -m "Pergament $version"
git push --atomic origin main "$tag"
echo "release: pushed $tag; the release workflow publishes it"
