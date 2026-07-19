#!/bin/sh
set -eu

PREFIX=${PREFIX:-/usr}
DESTDIR=${DESTDIR:-}
ROOT=${DESTDIR}${PREFIX}

if [ -z "$DESTDIR" ] && [ "$(id -u)" -ne 0 ]; then
  echo "uninstall.sh must run as root (or set DESTDIR for staging)." >&2
  exit 1
fi

rm -f "$ROOT/lib/jellyfin-content-filter/filterd"
rm -f "$ROOT/share/jellyfin-content-filter/jellyfin-content-filter.js"
rm -f "$ROOT/share/jellyfin-content-filter/jellyfin-content-filter-desktop.js"
rm -f "$ROOT/lib/systemd/system/jellyfin-content-filter.service"
rm -f "$ROOT/lib/sysusers.d/jellyfin-content-filter.conf"
rm -f "$ROOT/lib/tmpfiles.d/jellyfin-content-filter.conf"
rmdir "$ROOT/lib/jellyfin-content-filter" "$ROOT/share/jellyfin-content-filter" 2>/dev/null || true

echo "Program files removed. Configuration and state were intentionally preserved."
