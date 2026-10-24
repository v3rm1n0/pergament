#!/usr/bin/env bash
# Text that goes in front of the release notes GitHub generates (see
# .github/release.yml and the release workflow): how to install the downloads.
set -euo pipefail

echo 'For Linux, each release has an AppImage, a .deb, a .rpm and a .flatpak for x86_64, next to the zip. Install the .deb with `sudo apt install ./Pergament-X.Y.Z-linux-x86_64.deb` and the .rpm with `sudo dnf install ./Pergament-X.Y.Z-linux-x86_64.rpm`; they pull in what they need, including the H.264 video decoder. The .deb also adds the Pergament apt repository, so later versions arrive with the normal system update. The repository is signed with the key `71162C8E5F5EFABAA6ECB400A8001EBD72C3A8BF`. The AppImage runs after `chmod +x`. The flatpak installs with `flatpak install --user Pergament-X.Y.Z-linux-x86_64.flatpak`. Check every download against its `.sha256` file first. The zip stays for installs without a package manager; the README also describes building from source.'
echo
echo 'For Windows x86_64, extract `Pergament-X.Y.Z-windows-x86_64.zip` and run `pergament-app.exe`. The [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) is required and is already installed on most Windows systems. Check the zip against its `.sha256` file.'
echo
echo 'The `.dmg` is a build for Macs with Apple Silicon. It is not signed with an Apple Developer ID and not notarized, so macOS blocks the first start: right-click the app and choose Open, or allow it in System Settings under Privacy & Security. Check the download with `shasum -a 256 -c`. It has not been tested by the maintainer on a Mac.'
echo
# The in-app updater shows only what follows this marker.
echo '<!-- changes -->'
