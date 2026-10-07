#!/usr/bin/env bash
set -euo pipefail

# Pinned helper for Unix packaging/tests. The installed updater embeds HPatch instead.
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux64; sha=dcdae2056780c8b477b042d0db9e48226824ae87f94e724082e9c1dec9db8d1e ;;
  Linux-aarch64|Linux-arm64) platform=linux_arm64; sha=49eacbadce2702ed0654b8328d4c408eb2b855e3efad2f8315ec0aa1f266c8a6 ;;
  Darwin-*) platform=macos; sha=8e2acf400227c972ddef64235fc45630f2e1ab026de9676333453e798d9dba04 ;;
  *) echo 'Unsupported HDiffPatch build host' >&2; exit 1 ;;
esac
destination="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/velopack-hdiffpatch-4.12.0"
mkdir -p "$destination"
curl --fail --location --retry 3 \
  "https://github.com/sisong/HDiffPatch/releases/download/v4.12.0/hdiffpatch_v4.12.0_bin_${platform}.zip" \
  --output "$destination/download.zip"
echo "$sha  $destination/download.zip" | shasum -a 256 --check
unzip -o -q "$destination/download.zip" -d "$destination"
chmod +x "$destination/$platform/hdiffz"
if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "$destination/$platform" >> "$GITHUB_PATH"
else
  echo "Add $destination/$platform to PATH."
fi
