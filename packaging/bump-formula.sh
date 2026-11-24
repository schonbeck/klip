#!/bin/sh
# Point the Homebrew formula at a released version:
#   packaging/bump-formula.sh 0.1.0 path/to/homebrew-tap/Formula/klip.rb
# Downloads the release source tarball and writes its url and sha256.
set -eu
v="$1"
formula="$2"
url="https://github.com/schonbeck/klip/releases/download/v${v}/klip-${v}.tar.gz"
sha=$(curl -fsSL "$url" | sha256sum | cut -d' ' -f1)
sed -i -e "s|^  url \".*\"|  url \"${url}\"|" -e "s|^  sha256 \".*\"|  sha256 \"${sha}\"|" "$formula"
echo "klip ${v}: ${url} ${sha}"
