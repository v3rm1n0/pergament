# Contributing

Changes to `main` come in through pull requests; please do not push there
directly. Keep commits small and use conventional titles (`feat:`, `fix:`,
`docs:` ...). The title of a pull request ends up in the release notes, so make
it read well as one line.

## Running from source

```sh
nix develop          # or: direnv allow
bun install
bun run tauri dev
```

`bun run tauri build --debug --no-bundle` builds `target/debug/pergament-app`
with the frontend embedded. The flake has a dev shell and a package
(`nix build .#pergament`) with the desktop app `pergament-app`, its desktop
entry and icons, and the command line tool `pergament`. `nix run .#pergament`
starts the app.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun run lint
bun run test --run
bun run i18n
```

The Rust tests build small valid and malicious `.jwpub` files and user data
backups in memory. To also test against real publications, set
`PERGAMENT_TEST_JWPUB` (study Bible) and `PERGAMENT_TEST_JWPUB_WP` (a
Watchtower issue) to their paths; without them those tests are skipped.
`PERGAMENT_TEST_NETWORK=1` enables a test that downloads one Watchtower issue
(about 3 MB) from jw.org.

## Translations

Adding an interface language takes one file and one line, and `bun run i18n`
checks it. See [docs/TRANSLATING.md](docs/TRANSLATING.md).

## Releases

Versions follow [semantic versioning](https://semver.org). A release is made
through a pull request as well. On a clean, up-to-date `main` inside
`nix develop`, run `scripts/release.sh X.Y.Z`. It runs the checks, sets the
version in `Cargo.toml`, `src-tauri/Cargo.toml`, `package.json` and
`tauri.conf.json` on a branch `release/vX.Y.Z` and opens a pull request. Once
that is merged, run `scripts/release.sh --tag X.Y.Z` on `main`: it tags
`vX.Y.Z` and pushes the tag.

The tag starts a GitHub workflow that publishes the release with the Linux zip
and its checksum. The release notes are the ones GitHub generates from the
merged pull requests since the last release, with the install instructions in
front.
