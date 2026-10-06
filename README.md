# Pergament

Pergament is an unofficial Linux reader for publications in the `.jwpub`
format. It has a desktop app (Tauri, with a React frontend) that also builds for
Android and iOS, a command line
tool called `pergament`, and a Rust library underneath both. The app's layout
follows JW Library on other platforms, with its own name, colours and icons.

![Home with today's daily text](docs/screenshots/01-home-light.png)

| | |
|---|---|
| ![Daily text with a Bible reference open in the side pane](docs/screenshots/02-daily-text-reference-dark.png) | ![Hebrews 10 with highlights, a note and the study pane](docs/screenshots/03-bible-study-pane-dark.png) |
| ![Workbook week with highlights in all six colours](docs/screenshots/06-workbook-highlights-light.png) | ![Personal Study with notes and tags](docs/screenshots/08-personal-study-light.png) |

Every view is in [docs/screenshots](docs/screenshots), in both themes. The
notes and highlights in them are sample data.

## Before you use it

This project is not affiliated with, endorsed by or supported by Jehovah's
Witnesses or the Watch Tower Bible and Tract Society. "JW Library" and
related names are their trademarks.

It is meant for reading publications you have obtained yourself, on your own
computer. The repository contains no publications; you import or download
them at runtime. The only publication text and catalog images in it are what
the screenshots in `docs/screenshots` show. Test files are read from paths
you supply and are never committed.

Nothing was decompiled or taken from the official app. The file format was
worked out from public open-source projects and from inspecting `.jwpub`
files and one user data backup. Every detail and where it came from is in
[docs/FORMAT.md](docs/FORMAT.md).

Publication text inside `.jwpub` files is encrypted, and Pergament decrypts
it so you can read it. Depending on where you live, distributing software
that does this may be restricted. Read the [license notes](#license) before
you use or share it.

Pergament sends no telemetry. The only network traffic goes to jw.org's
public catalog and download services, and only when you search or download.
Imported files are treated as untrusted: hashes are verified, archives are
unpacked with path and size checks, and rendered HTML is sanitized.

## Desktop app

- Home shows today's date and theme scripture when the daily text booklet is
  downloaded. It opens a Daily Text view with arrows for the previous and
  next day. Below are your favorites, the Teaching Toolbox and What's New.
- The Bible has tabs from the publication's own navigation, book tiles and a
  chapter grid. The reader has a study pane with the outline, footnotes,
  cross references and study notes per verse.
- The Library lists the catalog's categories and your downloaded
  publications.
- Meetings shows the selected week's workbook program and Watchtower study
  article from downloaded issues, with links to download the rest.
- Links in a text open in a side pane with the cited verses or paragraphs
  marked. The publication bar at the top of the pane opens the reference in
  the main reader. If the publication isn't downloaded, the pane offers to
  download it.
- Select text to highlight it in one of six colours or attach a note. Click
  a highlight to change its colour, add a note or remove it.
- Personal Study lists your notes with their tags, and your bookmarks.
- Backups are `.jwlibrary` files, the same format JW Library uses, so a
  backup from a phone can be restored here and the other way round.
  Restoring replaces the current data; the previous database is kept as
  `userData.db.before-restore` in the library folder.

The language menu sits in the top right corner. There is a light and a dark
theme.

Home lists, categories and meetings come from jw.org's public catalog, about
58 MB. It is downloaded only when you ask for it and checked for updates at
most once a day. Cover images are fetched once and cached.

On Linux the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` (unless it is
already set) because WebKitGTK draws garbage on some GPUs without it.

To run it from the source tree:

```sh
nix develop
pnpm install
pnpm tauri dev
```

`pnpm tauri build --debug --no-bundle` builds `target/debug/pergament-app`
with the frontend embedded.

## Mobile apps

The desktop app also builds for Android and iOS from the same code (Tauri 2
mobile, Rust backend and React frontend). On a narrow screen the rail becomes a
tab bar at the bottom and the study pane covers the reader. The library lives
in the app's private folder. Pick `.jwpub` files and `.jwlibrary` backups with
the system file picker; they are copied in, so nothing else needs access to
your files.

Build with the Rust toolchain from rustup (the Nix shell's cargo has no mobile
targets) and `scripts/mobile.sh`, which adds the Rust targets and calls the
Tauri CLI:

```sh
# Android: JDK 17, Android SDK and NDK (r27 or newer)
export ANDROID_HOME=~/Android/Sdk NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
scripts/mobile.sh android --apk --debug      # unsigned debug APK
scripts/mobile.sh android --aab              # release bundle, needs your own signing setup

# iOS: macOS with Xcode and xcodegen (brew install xcodegen)
scripts/mobile.sh ios --debug --target aarch64-sim    # simulator
APPLE_DEVELOPMENT_TEAM=XXXXXXXXXX scripts/mobile.sh ios --export-method debugging
```

`pnpm tauri android dev` and `pnpm tauri ios dev` run the app on a device or
emulator against the Vite dev server. The Android project is in
`src-tauri/gen/android`; the Xcode project is generated on first iOS build and
not committed. The `Mobile` workflow builds an unsigned Android APK and an iOS
simulator app on demand and keeps them as workflow artifacts, nothing more.

Known limits: writing a backup (`Create backup`) needs a save dialog, which
the platforms may not offer; restoring one works. The mobile builds have not
been run on a device yet.

## Command line

```sh
pergament import nwtsty_X.jwpub wp_X_202609.jwpub   # verify and add to the library
pergament list                                      # show imported publications
pergament show nwtsty                               # list books and documents
pergament show nwtsty 19:23 > psalm23.html          # Bible chapter (book:chapter)
pergament show wp26 3 > article.html                # document by DocumentId
pergament search X wachtturm                        # search the catalog (X = German)
pergament download wp --lang X --issue 20260900     # download, verify, import
pergament download nwtsty --lang X                  # undated works need no issue
```

`show` writes a complete HTML page with light and dark styles, with images
pointing into the library. `--fragment` prints only the content.

Language codes are mapped to MEPS ids through the catalog; for a language
that can't be mapped, pass `--meps-id`. Downloads resume after an
interruption. They are checked against the MD5 from the download API and,
when the catalog is cached, against its size and SHA-1, and then go through
the normal import. Requests carry a `pergament/…` User-Agent, are at least
one second apart and are retried with backoff on network errors, 429 and
5xx.

The library lives in `$XDG_DATA_HOME/pergament` (usually
`~/.local/share/pergament`); `--library DIR` or `PERGAMENT_LIBRARY` changes
it. The catalog and partial downloads go to `$XDG_CACHE_HOME/pergament`
(`--cache DIR` or `PERGAMENT_CACHE`). A folder from before the rename,
`jwlinux`, is moved to the new name on first start. Importing a publication
again replaces the existing copy.

## Development

```sh
nix develop          # or: direnv allow
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint
pnpm test --run
```

The Rust tests build small valid and malicious `.jwpub` files and user data
backups in memory. To also test against real publications, set
`PERGAMENT_TEST_JWPUB` (study Bible) and `PERGAMENT_TEST_JWPUB_WP` (a
Watchtower issue) to their paths; without them those tests are skipped.
`PERGAMENT_TEST_NETWORK=1` enables a test that downloads one Watchtower issue
(about 3 MB) from jw.org.

The flake has a dev shell and a package (`nix build .#pergament`) with the
desktop app `pergament-app`, its desktop entry and icons, and the command line
tool `pergament`. `nix run .#pergament` starts the app.

## Releases

Versions follow [semantic versioning](https://semver.org). To release, run
`scripts/release.sh X.Y.Z` on a clean `main` inside `nix develop`. It runs
the checks, sets the version in `Cargo.toml`, `src-tauri/Cargo.toml`,
`package.json` and `tauri.conf.json`, commits, tags `vX.Y.Z` and pushes.
The tag starts a GitHub workflow that publishes the release with notes made
from the commit messages since the last tag. A release contains the source
code only.

## License

This project is free software under the GNU General Public License,
version 3 or (at your option) any later version. The full text is in
[LICENSE](LICENSE).

The notes below describe how the project is published and what you should
know before using it. They do not add restrictions to the GPL.

### Tolerated, not authorised

The project has no permission from the Watch Tower Bible and Tract Society
or any other rights holder of the publications it reads. At most, its
existence and use are tolerated. That can change at any time, and the
project may then be changed or withdrawn without notice.

### Use at your own risk

You use this software entirely at your own risk. As sections 15 and 16 of
the GPL state, it comes without any warranty, and the authors are not
liable for any damage or loss, including lost data and any legal
consequences of using it. You are responsible for checking that your use
is lawful where you live, in particular decrypting `.jwpub` files and
downloading from jw.org.

### Source code only

The project is published as uncompiled source code only. There are no
official builds, packages or installers. To use it, build it yourself as
described under [Desktop app](#desktop-app) and
[Command line](#command-line). Compiled versions from anyone else do not
come from this project and are not supported.
