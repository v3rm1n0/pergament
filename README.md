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

The library lives in `$XDG_DATA_HOME/jwlinux` (usually
`~/.local/share/jwlinux`). Override it with `--library DIR` or
`JWL_LIBRARY`. Re-importing a publication replaces the existing copy.

`jwl show` writes a complete HTML page with light and dark styles. Images
point to the files in the library. Use `--fragment` for just the content.

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
skipped when the variables are not set.

## License

Not decided yet.
