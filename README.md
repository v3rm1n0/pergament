# jwlinux

An unofficial, native Linux reader for JW publications in the `.jwpub`
format, written in Rust. It has a library crate (`jwlinux`) and a command
line tool (`jwl`). A GTK4/libadwaita interface is planned.

## Status and position

- **Unofficial.** This project is not affiliated with, endorsed by or
  supported by Jehovah's Witnesses or the Watch Tower Bible and Tract
  Society. "JW Library" and related names are their trademarks.
- **Personal use only.** It is meant for reading publications you have
  obtained yourself, on your own computer.
- **No bundled content.** The repository contains no publications, images
  or other content. You import your own `.jwpub` files at runtime. Test
  files are read from paths you supply and are never committed.
- **Clean room.** Nothing was decompiled or taken from the official app.
  The file format was worked out from public open-source projects and from
  inspecting `.jwpub` files; every detail and its source is listed in
  [docs/FORMAT.md](docs/FORMAT.md).
- **No telemetry.** Nothing is sent anywhere. All imported data is treated
  as untrusted: hashes are verified, archives are unpacked with path and
  size checks, and rendered HTML is sanitized.

Publication text inside `.jwpub` files is encrypted. jwlinux decrypts it so
you can read it. Depending on where you live, distributing software that
does this may be restricted. Check before you redistribute it.

## Usage

```sh
jwl import nwtsty_X.jwpub wp_X_202609.jwpub   # verify and add to the library
jwl list                                      # show imported publications
jwl show nwtsty                               # list books and documents
jwl show nwtsty 19:23 > psalm23.html          # Bible chapter (book:chapter)
jwl show wp26 3 > article.html                # document by DocumentId
```

Downloading from jw.org's public services:

```sh
jwl search X wachtturm                        # search the catalog (X = German)
jwl download wp --lang X --issue 20260900     # download, verify, import
jwl download nwtsty --lang X                  # undated works need no issue
```

`search` downloads the public publication catalog once (about 58 MB) and
checks for a newer one at most once a day (`--refresh` forces a check).
Language codes are mapped to MEPS ids from the catalog; for languages that
cannot be mapped, pass `--meps-id`. Downloads resume after interruptions, are
checked against the MD5 from the download API and, when the catalog is
cached, against its size and SHA-1, and then go through the normal import.
Requests carry a `jwlinux/…` User-Agent, are spaced at least one second apart
and are retried with backoff on network errors, 429 and 5xx.

The library lives in `$XDG_DATA_HOME/jwlinux` (usually
`~/.local/share/jwlinux`). Override it with `--library DIR` or
`JWL_LIBRARY`. The catalog and partial downloads are kept in
`$XDG_CACHE_HOME/jwlinux` (`--cache DIR` or `JWL_CACHE`). Re-importing a
publication replaces the existing copy.

`jwl show` writes a complete HTML page with light and dark styles. Images
point to the files in the library. Use `--fragment` for just the content.

## Desktop app

`jwlinux-gtk` is a GTK4/libadwaita reader for the same library:

- sidebar with your publications, their documents and, for Bibles, books
  and chapters; the reader shows pages in WebKitGTK with JavaScript off
- links work inside the app: Bible references open the chapter in your
  Bible, publication links open the document if it is in the library
  (otherwise you can open it on jw.org), web links open in your browser
- import `.jwpub` files (Ctrl+O), search and download online (Ctrl+F)
- follows the system light/dark style, or force one in the menu
- `jwlinux-gtk nwtsty 19:23` or `jwlinux-gtk wp26 3` opens a page directly

```sh
nix run .#jwlinux-gtk
```

## Development

```sh
nix develop          # or: direnv allow (uses the flake)
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests build small valid and malicious `.jwpub` files in memory. To also
test against real publications, set `JWL_TEST_JWPUB` (study Bible) and
`JWL_TEST_JWPUB_WP` (a Watchtower issue) to their paths. Those tests are
skipped when the variables are not set. `JWL_TEST_NETWORK=1` enables a test
that downloads one Watchtower issue (about 3 MB) from jw.org.

## License

Not decided yet.
