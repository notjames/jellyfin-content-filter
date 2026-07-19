# Installation

Jellyfin Content Filter has three required components:

1. the Rust `filterd` analysis and policy service;
2. the .NET Jellyfin server plugin;
3. one playback bundle selected for the client: Jellyfin Web or Linux Jellyfin Desktop.

Installing only the server components does not change playback. A client bundle is required to perform skips and mutes.

## Supported targets

| Target | Bundle | Requirements |
| --- | --- | --- |
| Jellyfin Web | `web/dist/jellyfin-content-filter.js` | Server-provided Jellyfin Web HTML5 player and JavaScript Injector |
| Linux Jellyfin Desktop | `desktop/dist/jellyfin-content-filter-desktop.js` | Current `jellyfin/jellyfin-desktop` CEF client built from recent `main`, plus JavaScript Injector |

The desktop bundle controls native mpv through the client-provided `window.api.player` bridge. It is separate from the Web bundle and does not require rebuilding Jellyfin Desktop.

Legacy Qt Jellyfin Media Player, mobile/TV applications, Roku, Kodi, Chromecast, SyncPlay, external players, and downloaded files are not supported. They bypass these playback adapters.

## Build requirements

- Jellyfin Server 10.11.x
- Rust 1.85 or newer
- .NET SDK 9
- Node.js 20 or newer and npm
- FFmpeg and ffprobe
- `zip`, `tar`, and standard Linux installation tools
- JavaScript Injector installed in Jellyfin
- Chromium for browser tests: `npx playwright install chromium`

Model weights are not bundled. Subtitle-based filtering works without a model. Speech and visual analysis require separately installed, license-reviewed Whisper and OpenVINO models.

## 1. Build and test

From the project root:

```sh
cargo test --workspace
cargo build --release

npm ci
npm test
npm run lint
npm run build:all
npm run test:e2e

dotnet test plugin/Jellyfin.Plugin.ContentFilter.Tests/Jellyfin.Plugin.ContentFilter.Tests.csproj
dotnet build plugin/Jellyfin.Plugin.ContentFilter/Jellyfin.Plugin.ContentFilter.csproj -c Release
```

Build the installable plugin ZIP:

```sh
VERSION=0.1.0 packaging/scripts/package-plugin.sh
```

or use `dotnet10` installed in `/usr/local/dotnet10`:

```bash
DOTNET=/usr/local/bin/dotnet10 VERSION=0.1.0 \
    packaging/scripts/package-plugin.shim
```

The ZIP is written to `dist/jellyfin-content-filter-plugin-0.1.0.zip`.

## 2. Install the Rust service

Inspect `packaging/config/filterd.env.example`. Generate a long random shared token:

```sh
openssl rand -hex 32
```

Install the files as root:

```sh
packaging/scripts/install.sh
```

Edit `/etc/jellyfin-content-filter/filterd.env`:

- replace `REPLACE_ME` in `FILTERD_AUTH_TOKEN`;
- verify the FFmpeg paths;
- keep `FILTERD_ANALYSIS_CONCURRENCY=1` initially so Intel GPU analysis does not starve Jellyfin transcoding;
- optionally configure Whisper and OpenVINO executables and model paths.

Keep this file root-owned with mode `0600`.

The install script places account and directory declarations at:

```text
/usr/lib/sysusers.d/jellyfin-content-filter.conf
/usr/lib/tmpfiles.d/jellyfin-content-filter.conf
```

Apply those declarations explicitly:

```sh
sudo systemd-sysusers /usr/lib/sysusers.d/jellyfin-content-filter.conf
sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/jellyfin-content-filter.conf
```

This creates the `jellyfin-content-filter` system user and the persistent `/var/lib/jellyfin-content-filter` and `/etc/jellyfin-content-filter` directories. systemd creates `/run/jellyfin-content-filter` whenever the service starts.

Verify and start the service:

```sh
getent passwd jellyfin-content-filter
id jellyfin-content-filter
sudo systemctl daemon-reload
sudo systemctl enable --now jellyfin-content-filter.service
sudo systemctl status jellyfin-content-filter.service
```

The daemon listens only on `/run/jellyfin-content-filter/filterd.sock`.

## 3. Install the Jellyfin server plugin

Install `dist/jellyfin-content-filter-plugin-0.1.0.zip` using Jellyfin's supported plugin installation workflow, then restart Jellyfin. A manual installation is expected at `/var/lib/jellyfin/plugins/Content Filter_0.1.0/Jellyfin.Plugin.ContentFilter.dll`.

Because this development build is not published in a remote Jellyfin catalog, Jellyfin 10.11's unified plugin-details page may report that it cannot get repository details even when the assembly loaded successfully. Open the configuration page directly:

```text
http://YOUR_SERVER:8096/web/#/configurationpage?name=ContentFilter
```

Then:

1. set the socket to `/run/jellyfin-content-filter/filterd.sock`;
2. enter the same token used for `FILTERD_AUTH_TOKEN`;
3. leave **Fail open** enabled;
4. enable scheduled analysis if desired;
5. save and verify daemon health.

The server plugin authenticates viewers, checks library visibility, schedules analysis, and exposes the review interface. It never modifies media files.

## 4. Install JavaScript Injector

For Jellyfin 10.11:

1. Open **Dashboard → Plugins → Catalog**.
2. Open the repository settings using the gear icon.
3. Add a repository named `JavaScript Injector`.
4. Use this exact JSON manifest URL:

   ```text
   https://raw.githubusercontent.com/n00bcodr/jellyfin-plugins/main/10.11/manifest.json
   ```

5. Save, return to **Catalog**, find **JavaScript Injector**, and install it.
6. Restart Jellyfin:

   ```sh
   sudo systemctl restart jellyfin
   ```

7. Open **Dashboard → Plugins → JavaScript Injector**.
8. Add an enabled script named `Content Filter Web`, mark it **Requires Authentication**, and paste the complete contents of:

   ```text
   web/dist/jellyfin-content-filter.js
   ```

9. Add a second enabled, authenticated script named `Content Filter Desktop` and paste:

   ```text
   desktop/dist/jellyfin-content-filter-desktop.js
   ```

10. Save and fully reload Web clients or restart Jellyfin Desktop.

Paste only the built JavaScript files, not TypeScript source files. Registering both bundles is safe: their runtime guards select the appropriate client target and prevent duplicate schedulers.

## 5A. Jellyfin Web clients

Register:

```text
web/dist/jellyfin-content-filter.js
```

Reload Jellyfin Web. Start a movie or episode and verify:

- a **Content filter** button appears;
- `window.JellyfinContentFilter.target` is `web`;
- ordinary playback remains Direct Play/Direct Stream when it did before installation.

The Web bundle intentionally refuses to start when it detects Jellyfin Desktop.

## 5B. Linux Jellyfin Desktop

For the current CEF-based `jellyfin/jellyfin-desktop` client, register:

```text
desktop/dist/jellyfin-content-filter-desktop.js
```

Completely close and reopen Jellyfin Desktop after changing injected scripts. Start playback and verify:

- a **Content filter** button appears;
- `window.JellyfinContentFilter.target` is `desktop`;
- skips call native mpv seeking;
- audio returns to its previous mute state after a mute interval.

Do not use the Web bundle as the desktop implementation. If both bundles are accidentally available, desktop detection prevents the Web scheduler from starting and the global startup guard prevents duplicate schedulers.

The desktop target is tied to the compatibility bridge in current Jellyfin Desktop `main`. Retest after desktop-client updates that change `native-shim.js` or `mpv-player-base.js`.

## 6. Analyze and review media

Run **Analyze library for content filtering** from Jellyfin scheduled tasks or enqueue an individual item through the plugin API. Machine-generated findings are pending candidates and do not filter playback until approved.

Open **Content Filter Review** in the Jellyfin dashboard to:

- inspect transcript or model evidence;
- edit start/end timestamps, category, severity, and action;
- approve or reject each candidate.

Viewer and per-title choices are available from the in-player **Content filter** button.

## Verification and troubleshooting

Service logs:

```sh
journalctl -u jellyfin-content-filter.service
```

Common issues:

- **No client button:** verify JavaScript Injector is enabled, the correct target bundle is registered, and the client was fully reloaded.
- **Daemon unavailable:** confirm the socket path, shared token, service status, and that Jellyfin can access the `jellyfin` group-owned socket.
- **No detections:** configure profanity terms and ensure subtitle files are readable; configure Whisper/OpenVINO separately for speech or visual analysis.
- **Desktop skips do nothing:** confirm this is the current CEF `jellyfin/jellyfin-desktop`, not the retired Qt Jellyfin Media Player.
- **Filtering unexpectedly stops:** the system deliberately fails open. Check server, injector, and daemon logs.

Browser-side filtering is not tamper-resistant parental control. Disabling JavaScript, using unsupported clients, casting, downloading, or accessing stream URLs bypasses it.

## Upgrade and removal

Before upgrading, back up:

```text
/etc/jellyfin-content-filter/filterd.env
/var/lib/jellyfin-content-filter/filterd.sqlite3
```

Rebuild both bundles and the plugin against the new supported Jellyfin versions, replace the installed artifacts, restart the daemon and Jellyfin, then retest playback.

To remove program files while preserving configuration and analysis state:

```sh
systemctl disable --now jellyfin-content-filter.service
packaging/scripts/uninstall.sh
```

Also remove the Jellyfin plugin and JavaScript Injector entries. Delete `/etc/jellyfin-content-filter` and `/var/lib/jellyfin-content-filter` only if configuration, policies, reviews, and analysis history are no longer needed.
