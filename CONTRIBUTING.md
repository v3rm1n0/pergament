# Contributing

Changes to `main` come in through pull requests; please do not push there
directly. Keep commits small and use conventional titles (`feat:`, `fix:`,
`docs:` ...). The title of a pull request ends up in the release notes, so make
it read well as one line.

See [Platform maintainers](MAINTAINERS.md) for Windows maintainer permissions,
Scoop bucket requirements, and the Windows portable smoke test.

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
`PERGAMENT_TEST_JWPUB` (study Bible), `PERGAMENT_TEST_JWPUB_WP` (a
Watchtower issue), and `PERGAMENT_TEST_JWPUB_LFF` (Enjoy Life Forever) to their
paths; without them the corresponding tests are skipped.
`PERGAMENT_TEST_NETWORK=1` enables a test that downloads one Watchtower issue
(about 3 MB) from jw.org.

## Screenshots

The images in `docs/screenshots` show the real interface in headless Chromium.
The Tauri backend is replaced by `scripts/screenshots/mock.js`, and everything
in it is invented. Do not put real publication text, wording or artwork into
the mock or the images, and do not take screenshots of a real library.

```sh
bun run dev        # in another terminal, serves the UI on port 1420
node scripts/screenshots/run.mjs --out /tmp/shots
```

Playwright is not a dependency of the project. Set `PLAYWRIGHT_RESOLVE_FROM`
to a directory whose `node_modules` has `playwright` or `playwright-core`, and
`CHROMIUM_PATH` if Playwright's own browser does not run (on NixOS, take the
one from `nix shell nixpkgs#chromium`). The date is fixed, so a run gives the
same pictures. Look at every image in the scratch directory first, then run
again without `--out` to replace the ones in `docs/screenshots`.

## Translations

Adding an interface language takes one file and one line, and `bun run i18n`
checks it. See [docs/TRANSLATING.md](docs/TRANSLATING.md).

## Releases

Versions follow [semantic versioning](https://semver.org). A release is made
through a pull request as well. On a clean, up-to-date `main` inside
`nix develop`, run `scripts/release.sh X.Y.Z`. It runs the checks, sets the
version in `Cargo.toml`, `src-tauri/Cargo.toml`, `package.json` and
`tauri.conf.json` and adds the release to the AppStream metainfo, on a branch
`release/vX.Y.Z`, and opens a pull request. Once that is merged, run
`scripts/release.sh --tag X.Y.Z` on `main`: it tags `vX.Y.Z` and pushes the
tag.

The tag starts a GitHub workflow that publishes the release with the Linux
zip, the .deb, .rpm and AppImage packages and the flatpak bundle, each with
its checksum, and later adds a disk image for Apple Silicon Macs. The release
notes are the ones GitHub generates from the merged pull requests since the
last release, with the install instructions in front. Before tagging, test
the formats from a `workflow_dispatch` run of the workflow, and check them by
hand: install the .deb on Ubuntu and the .rpm on Fedora, run the AppImage,
install the flatpak, and in each start the app from the menu, run `pergament`
and `pergament-app`, and play a public video with sound (the H.264 check).
In the flatpak, also check the library location and
`flatpak run --command=pergament io.github.v3rm1n.pergament`.
