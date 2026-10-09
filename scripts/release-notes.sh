#!/usr/bin/env bash
# Text that goes in front of the release notes GitHub generates (see
# .github/release.yml and the release workflow): how to install the zip.
set -euo pipefail

echo 'Download the zip for Linux, unpack it and run `./install.sh`. It installs the binaries and a desktop entry below `~/.local`. Check the download against the `.sha256` file with `sha256sum -c`. The README also describes building from source.'
