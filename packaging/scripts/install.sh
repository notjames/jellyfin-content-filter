#!/bin/sh
set -eu

PREFIX=${PREFIX:-/usr}
DESTDIR=${DESTDIR:-}
ROOT=${DESTDIR}${PREFIX}
ETC=${DESTDIR}/etc
STATE=${DESTDIR}/var/lib/jellyfin-content-filter

if [ -z "$DESTDIR" ] && [ "$(id -u)" -ne 0 ]; then
  echo "install.sh must run as root (or set DESTDIR for staging)." >&2
  exit 1
fi
for file in target/release/filterd web/dist/jellyfin-content-filter.js desktop/dist/jellyfin-content-filter-desktop.js; do
  if [ ! -f "$file" ]; then
    echo "Missing $file; build release artifacts first." >&2
    exit 1
  fi
done

install -d -m 0755 "$ROOT/lib/jellyfin-content-filter" "$ROOT/share/jellyfin-content-filter"
install -m 0755 target/release/filterd "$ROOT/lib/jellyfin-content-filter/filterd"
install -m 0644 web/dist/jellyfin-content-filter.js "$ROOT/share/jellyfin-content-filter/"
install -m 0644 desktop/dist/jellyfin-content-filter-desktop.js "$ROOT/share/jellyfin-content-filter/"
install -d -m 0750 "$ETC/jellyfin-content-filter" "$STATE"
if [ ! -e "$ETC/jellyfin-content-filter/filterd.env" ]; then
  install -m 0600 packaging/config/filterd.env.example "$ETC/jellyfin-content-filter/filterd.env"
fi
install -D -m 0644 packaging/systemd/jellyfin-content-filter.service "$ROOT/lib/systemd/system/jellyfin-content-filter.service"
install -D -m 0644 packaging/systemd/jellyfin-content-filter.sysusers "$ROOT/lib/sysusers.d/jellyfin-content-filter.conf"
install -D -m 0644 packaging/systemd/jellyfin-content-filter.tmpfiles "$ROOT/lib/tmpfiles.d/jellyfin-content-filter.conf"

echo "Installed under ${DESTDIR:-/}. Review config, install the Jellyfin plugin, then enable the service."
