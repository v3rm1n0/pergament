# Pergament

Pergament is an unofficial Linux reader for publications in the `.jwpub`
format. It has a desktop app (Tauri, with a React frontend), a command line
tool called `pergament`, and a Rust library underneath both. The app's layout
follows JW Library on other platforms, with its own name, colours and icons.

![Home with today's daily text](docs/screenshots/01-home-light.png)

| | |
|---|---|
| ![Daily text with a Bible reference open in the side pane](docs/screenshots/02-daily-text-reference-dark.png) | ![Hebrews 10 with highlights, a note and the study pane](docs/screenshots/03-bible-study-pane-dark.png) |
| ![Workbook week with highlights in all six colours](docs/screenshots/06-workbook-highlights-light.png) | ![Personal Study with notes and tags](docs/screenshots/08-personal-study-light.png) |

Every view is in [docs/screenshots](docs/screenshots), in both themes. They
show English publications; the notes and highlights are sample data.

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

Pergament is free and non-commercial, and it will stay that way. It only
downloads files that jw.org offers for download, such as `.jwpub` publications
and MP3 and MP4 recordings, from the same public services the site uses. It
does not scrape pages, and it is not meant for commercial use.

Pergament sends no telemetry. The only network traffic goes to jw.org's
public catalog and download services, and only when you search, download or
play a recording.
Imported files are treated as untrusted: hashes are verified, archives are
unpacked with path and size checks, and rendered HTML is sanitized.

## Install

Pergament is released as a zip for Linux, or you build it from source on your
machine.

**From the release zip.** Download `pergament-X.Y.Z-linux-x86_64.zip` and its
`.sha256` file from the [releases page](https://github.com/v3rm1n0/pergament/releases),
then:

```sh
sha256sum -c pergament-X.Y.Z-linux-x86_64.zip.sha256
unzip pergament-X.Y.Z-linux-x86_64.zip
cd pergament-X.Y.Z-linux-x86_64
./install.sh                    # --prefix DIR, --dry-run and --uninstall also work
```

The installer copies `pergament-app` and `pergament` to `~/.local/bin`, adds
the icons and a desktop entry so Pergament shows up in your application menu,
and checks that the libraries it needs (WebKitGTK 4.1, GTK 3, libsoup 3,
OpenSSL, SQLite) are installed. The zip is built on Ubuntu 22.04, so it needs
a distribution at least that recent.

Playing videos needs GStreamer with an H.264 decoder (`gst-libav` on Debian
and Ubuntu, `gstreamer1-libav` on Fedora, `gst-libav` on Arch). Without it the
player offers to open the recording in your browser instead.

To build from source there are two ways.

**With Nix** (flakes enabled), straight from GitHub:

```sh
nix profile install github:v3rm1n0/pergament   # installs pergament-app and pergament
nix run github:v3rm1n0/pergament               # or just try the app once
```

**With the install script**, which uses Nix if you have it and otherwise
builds with cargo and bun and installs below `~/.local`:

```sh
git clone https://github.com/v3rm1n0/pergament.git
cd pergament
scripts/install.sh              # --prefix DIR, --no-nix, --dry-run and --uninstall also work
```

Without Nix it needs cargo, bun, pkg-config and the development files for
WebKitGTK 4.1, GTK 3, libsoup 3, OpenSSL and SQLite; the script lists what is
missing for Debian, Fedora and Arch. It also runs straight from the web
(`curl -fsSL https://raw.githubusercontent.com/v3rm1n0/pergament/main/scripts/install.sh | bash`).

> **Always check scripts you copy from the internet.** Piping a download into
> `bash` runs whatever it contains, with your permissions. Read
> [scripts/install.sh](scripts/install.sh) first, or clone the repository as
> above and run it from the checkout. The script is short, builds from source
> and only writes below the prefix (`~/.local` by default) or into your Nix
> profile. `--dry-run` prints every step without doing it.

## Desktop app

- Home shows today's date and theme scripture when the daily text booklet is
  downloaded. It opens a Daily Text view with arrows for the previous and
  next day. Below are your favorites, the Teaching Toolbox and What's New.
- The Bible has tabs from the publication's own navigation, book tiles and a
  chapter grid. The reader has a study pane with the outline, footnotes,
  cross references and study notes per verse.
- The Library lists the catalog's categories. Each category is split the way
  JW Library does it: newer and older publications, yearbooks, editions,
  Watchtower and Awake! by year. Downloaded publications are grouped by the
  type in their manifest, so types the catalog doesn't list, such as
  Curriculum and Outlines, show up once you import one.
- The Video and Audio tabs of the Library list the public video and audio
  categories. A click on a thumbnail streams the recording in a dialog that
  uses the system's own media controls, with quality, speed, repeat and
  subtitles. The cloud button downloads a quality you pick; downloads are
  verified, kept in the library folder and listed under Downloaded. Videos
  and audio that publications link to open in the same dialog.
- Meetings shows the selected week's workbook program and Watchtower study
  article from downloaded issues, with links to download the rest.
- A Bible link opens "Parallel Translations" in a side pane: the cited verses
  in every Bible of your library. "Customize" chooses which Bibles show and
  in what order. Other links open the reference in the pane with the cited
  paragraphs tinted. The publication bar at the top of the pane opens it in
  the main reader. If the publication isn't downloaded, the pane offers to
  download it.
- Answer fields in workbooks and study articles are text boxes. What you type
  is kept on this computer.
- Select text to highlight it in one of six colours or attach a note. Click
  a highlight to change its colour, add a note or remove it.
- Personal Study lists your notes with their tags, and your bookmarks.
- Backups are `.jwlibrary` files, the same format JW Library uses, so a
  backup from a phone can be restored here and the other way round.
  Restoring replaces the current data; the previous database is kept as
  `userData.db.before-restore` in the library folder.

The publication language menu sits in the top right corner. The interface
is available in English and German (Settings, by default it follows the
system language), and there is a light and a dark theme.

Home lists, categories and meetings come from jw.org's public catalog, about
58 MB. It is downloaded only when you ask for it and checked for updates at
most once a day. Cover images are fetched once and cached.

On Linux the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` (unless it is
already set) because WebKitGTK draws garbage on some GPUs without it.
It also sets `GST_PLUGIN_FEATURE_RANK` (unless it is already set) so GStreamer
does not pick NVIDIA's hardware video decoders, which take the page down on
NVIDIA under Wayland. Software decoding handles these videos fine.

To run it from the source tree without installing:

```sh
nix develop
bun install
bun run tauri dev
```

`bun run tauri build --debug --no-bundle` builds `target/debug/pergament-app`
with the frontend embedded.

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
bun run lint
bun run test --run
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
version 3 only. The full text is in
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

### Releases

Each release on GitHub has a zip with the compiled binaries, the icons and
an install script, a `.sha256` file to check the zip, and the source code.
These are the only official builds; there are no distribution packages. You
can also build it yourself as described under [Install](#install),
[Desktop app](#desktop-app) and [Command line](#command-line). Compiled
versions from anyone else do not come from this project and are not
supported.
