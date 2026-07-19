#!/bin/sh
set -eu

VERSION=${VERSION:?Set VERSION}
ARCH=${ARCH:-$(uname -m)}
OUT=${OUT:-dist}
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM

mkdir -p "$OUT"
DESTDIR="$STAGE" packaging/scripts/install.sh
tar -C "$STAGE" -czf "$OUT/jellyfin-content-filter-${VERSION}-${ARCH}.tar.gz" .
sha256sum "$OUT/jellyfin-content-filter-${VERSION}-${ARCH}.tar.gz" > "$OUT/jellyfin-content-filter-${VERSION}-${ARCH}.tar.gz.sha256"
echo "Created native archive in $OUT"
