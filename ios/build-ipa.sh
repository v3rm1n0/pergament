#!/usr/bin/env bash
# Builds an unsigned release IPA with xtool and writes it to xtool/Pergament.ipa.
set -euo pipefail

cd "$(dirname "$0")"
xtool dev build --configuration release --ipa
