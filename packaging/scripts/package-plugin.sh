#!/bin/sh
set -eu

VERSION=${VERSION:?Set VERSION}
DOTNET=${DOTNET:-dotnet}
OUT=${OUT:-dist}
PROJECT=plugin/Jellyfin.Plugin.ContentFilter/Jellyfin.Plugin.ContentFilter.csproj
ASSEMBLY=plugin/Jellyfin.Plugin.ContentFilter/bin/Release/net9.0/Jellyfin.Plugin.ContentFilter.dll
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM

"$DOTNET" build "$PROJECT" -c Release
test -f "$ASSEMBLY"
install -m 0644 "$ASSEMBLY" "$STAGE/Jellyfin.Plugin.ContentFilter.dll"
mkdir -p "$OUT"
OUT_ABS=$(cd "$OUT" && pwd)
(cd "$STAGE" && zip -q "$OUT_ABS/jellyfin-content-filter-plugin-${VERSION}.zip" ./*)
sha256sum "$OUT/jellyfin-content-filter-plugin-${VERSION}.zip" \
  > "$OUT/jellyfin-content-filter-plugin-${VERSION}.zip.sha256"
echo "Created Jellyfin plugin archive in $OUT"
