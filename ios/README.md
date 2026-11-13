# Pergament for iOS (experimental)

An unstable, early port. It is a `WKWebView` shell around the desktop
frontend, built with [xtool](https://xtool.sh) instead of Xcode. The Rust
core (`app-core`, linked through `ios/core`) runs in the app and answers the
UI's commands. Folder sync, background downloads and in-app updates are not
available on iOS.

## Build

Needs xtool with the Darwin SDK installed (`xtool setup`) and `bun`.

```sh
./ios/sync-web.sh       # builds the frontend into ios/Sources/Pergament/web
./ios/build-core.sh     # builds the Rust core into ios/lib (enters `nix develop` itself)
cd ios && xtool dev     # builds, installs and launches on a connected device
```

Which display modes iOS gives the app depends on the SDK version stamped into
the binary, not on the deployment target (iOS 17). `Package.swift` makes the
linker stamp the installed SDK's version (`LC_BUILD_VERSION` in the binary);
without that it stamps 17.0 and the app runs in a compatibility mode.

## How it works

- `Sources/Pergament/WebShell.swift` serves the bundled `web/` folder on
  `pergament://localhost` through a `WKURLSchemeHandler`, so the page has one
  origin and the absolute `/assets/...` paths from Vite work.
- `Sources/Pergament/Resources/tauri-shim.js` is injected at document start and
  stands in for `window.__TAURI_INTERNALS__`.
- The app icon is `AppIcon.png` (1024 px, opaque), named by `iconPath` in
  `xtool.yml`; xtool copies it into the bundle and sets `CFBundleIconFile`.

## Next

- Check the app on a device; nothing has run on hardware yet (blocked by pairing).
- Folder sync on iOS: needs security-scoped bookmarks through a document picker.
- Clipboard and other secure-context APIs: `pergament://` is not a secure context, so `navigator.clipboard` is undefined until a native bridge exists.
- Signing and App Store distribution.
