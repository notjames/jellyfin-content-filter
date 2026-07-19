# Installation

## Browser client

1. Run `npm ci && npm run build`.
2. Install and enable Jellyfin's JavaScript Injector plugin.
3. Make `web/dist/jellyfin-content-filter.js` available to Jellyfin Web (a same-origin URL is preferred).
4. Add an injector entry that loads that script after the document starts. Do not paste source modules; use the built IIFE.
5. Restart Jellyfin Web clients and verify the **Content filter** button appears during playback.

The script preserves deployments such as `https://host.example/jellyfin` by deriving the API base from observed PlaybackInfo URLs. Content Filter plugin routes must remain same-origin and enforce Jellyfin authentication.

## Linux Jellyfin Desktop

Build the independent native-mpv target with `npm run build:desktop`, then register `desktop/dist/jellyfin-content-filter-desktop.js` through JavaScript Injector for the current `jellyfin/jellyfin-desktop` client. Do not substitute the Web bundle: it intentionally abstains when the desktop bridge is present. No desktop-client rebuild is required. See [desktop.md](desktop.md) for the bridge and compatibility boundary.

## Native service

Build the release service and browser bundle first:

```sh
cargo build --release
npm ci
npm run build
```

Review `packaging/config/filterd.env.example`, especially the shared secret. To inspect installation without root:

```sh
DESTDIR=/tmp/jcf-stage packaging/scripts/install.sh
```

For a system installation, run `packaging/scripts/install.sh` as root yourself. The script does not invoke sudo, overwrite an existing configuration, enable services, or start processes. Then:

1. Generate and set `FILTERD_AUTH_TOKEN` in `/etc/jellyfin-content-filter/filterd.env`; keep the root-owned file at mode `0600`.
2. Build the plugin with .NET 9 and run `VERSION=0.1.0 DOTNET=/path/to/dotnet packaging/scripts/package-plugin.sh`.
3. Install the resulting plugin ZIP using your server's supported plugin workflow, then configure the same token and `/run/jellyfin-content-filter/filterd.sock` in its dashboard page.
4. Run `systemd-sysusers` and `systemd-tmpfiles --create`.
5. Review the systemd sandbox for local requirements.
6. Enable and start `jellyfin-content-filter.service`.

## Release artifacts

Set `VERSION`, build release artifacts, then run both packaging scripts. Catalog generation additionally requires `TARGET_ABI`, `SOURCE_URL`, and the plugin ZIP's SHA-256 `CHECKSUM`:

```sh
VERSION=1.0.0 TARGET_ABI=10.11.11.0 SOURCE_URL=https://example/plugin.zip \
CHECKSUM=<sha256> npm run catalog
```

Never publish placeholder secrets or an unverified checksum.
