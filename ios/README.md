# Pergament for iOS (experimental)

An unstable, early port. It is a `WKWebView` shell around the desktop
frontend, built with [xtool](https://xtool.sh) instead of Xcode. There is no
Rust core yet: the UI renders, but every command fails with "not available on
iOS yet".

## Build

Needs xtool with the Darwin SDK installed (`xtool setup`) and `bun`.

```sh
./ios/sync-web.sh       # builds the frontend into ios/Sources/Pergament/web
./ios/build-core.sh     # builds the Rust core into ios/lib (enters `nix develop` itself)
cd ios && xtool dev     # builds, installs and launches on a connected device
```

## How it works

- `Sources/Pergament/WebShell.swift` serves the bundled `web/` folder on
  `pergament://localhost` through a `WKURLSchemeHandler`, so the page has one
  origin and the absolute `/assets/...` paths from Vite work.
- `Sources/Pergament/Resources/tauri-shim.js` is injected at document start and
  stands in for `window.__TAURI_INTERNALS__`.
- The app icon is `AppIcon.png` (1024 px, opaque), named by `iconPath` in
  `xtool.yml`; xtool copies it into the bundle and sets `CFBundleIconFile`.

## Next

- Cross-compile the `pergament` crate for `aarch64-apple-ios` and route
  `invoke` to it over a C FFI or a `WKScriptMessageHandler`.
- Clipboard and other secure-context APIs: `pergament://` is not a secure context, so `navigator.clipboard` is undefined until a native bridge exists.
- Sheet transition for the reader's study pane; the compact layout has no animation yet.
