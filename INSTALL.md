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

sudo install -m 0755 \
  "$HOME"/projects/src/personal/jellyfin-content-filter/target/release/filterd \
  /usr/lib/jellyfin-content-filter/filterd
sudo install -m 0755 \
  "$HOME"/projects/src/personal/jellyfin-content-filter/target/release/openvino-runner \
  /usr/lib/jellyfin-content-filter/openvino-runner

# if you've already installed this and are just updating it, you may need
# to restart the service to pick up the new binary:
sudo systemctl restart jellyfin-content-filter

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
- for OpenVINO IR, set `FILTERD_OPENVINO_MODEL` to the `.xml` path (sibling `.bin` required); for ONNX, point at the `.onnx` file.
- optionally set `FILTERD_OPENVINO_LABELS` to a class→category JSON map (see `crates/openvino-runner/examples/labels-nsfw-binary.json`).

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

This creates the `jellyfin` runtime identity used by the service (media sidecars must be writable next to library files) and the persistent `/var/lib/jellyfin-content-filter` and `/etc/jellyfin-content-filter` directories. systemd creates `/run/jellyfin-content-filter` whenever the service starts.

On startup, `filterd` requeues orphaned `running` jobs, collapses duplicate `queued` rows (one per `media_id`), and drains the backlog through the analysis dispatcher. Restarts no longer leave the queue stranded.

The unit runs as `User=jellyfin` with `ProtectSystem=strict` and `ReadWritePaths=/nas`. Without `ReadWritePaths` for your media mounts, analysis finishes but writing `*.filter.json` fails with `Read-only file system (os error 30)`. If your library lives somewhere other than `/nas`, add that path to `ReadWritePaths` (or a drop-in override).

Verify and start the service:

```sh
id jellyfin
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

### 4.1 Install File Transformation

JavaScript Injector works most reliably through File Transformation, which avoids modifying `index.html` and avoids races with plugins such as Jellyfin Enhanced. File Transformation releases are tied to exact Jellyfin patch versions.

For Jellyfin Server `10.11.11.0`, manually install File Transformation `2.5.11.0`:

```sh
curl -fL \
  https://github.com/IAmParadox27/jellyfin-plugin-file-transformation/releases/download/2.5.11.0/Release-10.11.11.zip \
  -o /tmp/Release-10.11.11.zip

echo "1ebb39a6ad9c4b738638c5e8e5b7e3c67167a7163cf9e035433329739746251c  /tmp/Release-10.11.11.zip" \
  | sha256sum -c -

sudo systemctl stop jellyfin
sudo install -d -o jellyfin -g jellyfin \
  "/var/lib/jellyfin/plugins/File Transformation_2.5.11.0"
sudo unzip -o /tmp/Release-10.11.11.zip \
  -d "/var/lib/jellyfin/plugins/File Transformation_2.5.11.0"
sudo chown -R jellyfin:jellyfin \
  "/var/lib/jellyfin/plugins/File Transformation_2.5.11.0"
sudo systemctl start jellyfin
```

Check that the log reports File Transformation as loaded. If Jellyfin reports that it will become available after another restart, restart Jellyfin once more.

### 4.2 Install JavaScript Injector

For Jellyfin 10.11:

1. Open **Dashboard → Plugins → Catalog**.
2. Open the repository settings using the gear icon.
3. Add a repository named `JavaScript Injector`.
4. Use this exact JSON manifest URL:

   ```text
   https://raw.githubusercontent.com/n00bcodr/jellyfin-plugins/main/10.11/manifest.json
   ```

5. Save, return to **Catalog**, find **JavaScript Injector**, and install it.
6. Restart Jellyfin and confirm JavaScript Injector starts without errors:

   ```sh
   sudo systemctl restart jellyfin
   journalctl -u jellyfin --since "2 minutes ago" --no-pager |
     grep -E "File Transformation|JavaScript Injector"
   ```

   With File Transformation active, `/usr/share/jellyfin/web/index.html` should remain root-owned and does not need to be writable by Jellyfin:

   ```sh
   sudo chown root:root /usr/share/jellyfin/web/index.html
   sudo chmod 0644 /usr/share/jellyfin/web/index.html
   ```

7. Open **Dashboard → Plugins → JavaScript Injector**.
8. Add an enabled script named `Content Filter Web`, leave **Requires Authentication** unchecked, and paste the complete JavaScript source contained inside:

   ```text
   web/dist/jellyfin-content-filter.js
   ```

   Do not type or paste the filename itself. Open the file in an editor, select all of its contents, and paste that code into the injector's **Script** field.

9. Add a second enabled script named `Content Filter Desktop`, also leave **Requires Authentication** unchecked, and paste the complete contents—not the filename—of:

   ```text
   desktop/dist/jellyfin-content-filter-desktop.js
   ```

10. Save and fully reload Web clients or restart Jellyfin Desktop.

Paste only the built JavaScript files, not TypeScript source files. These bundles contain no tokens or private data; the server API remains authenticated. Registering both bundles is safe: their runtime guards select the appropriate client target and prevent duplicate schedulers.

## 5A. Verify Jellyfin Web

There is nothing else to install here if you pasted the `Content Filter Web` script in step 4.

1. Open Jellyfin in a normal web browser.
2. Force-reload it with `Ctrl+Shift+R`.
3. Start a movie or episode.
4. Verify that a **Content filter** button appears and ordinary playback still works.

For optional diagnostics, open the browser developer console and evaluate:

```js
window.JellyfinContentFilter
```

It should report `{ started: true, target: "web" }`.

## 5B. Verify Linux Jellyfin Desktop

There is nothing else to install here if you pasted the `Content Filter Desktop` script in step 4.

1. Completely close Jellyfin Desktop, including any background process.
2. Reopen it and connect to the server.
3. Start a movie or episode.
4. Verify that a **Content filter** button appears and playback still uses native mpv normally.

Actual skip and mute behavior can be tested after at least one candidate interval has been analyzed and approved. The desktop target is tied to the compatibility bridge in current Jellyfin Desktop `main` and should be retested after major desktop-client updates.

## 6. Analyze and review media

Run **Analyze library for content filtering** from Jellyfin scheduled tasks (or wait for automatic scans when titles are added). Each analyzed title gets a sidecar `Title.filter.json` next to the media file with raw detections. Playback uses those detections plus the viewer's mute/skip preferences. If analysis is incomplete (`partial`/`failed`), viewers who enabled filtering see a warning and playback continues unfiltered; problem details are stored for administrator investigation via `GET /ContentFilter/problems`.

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
- **Injector reports success but no button appears:** another plugin that rewrites `index.html` (for example Jellyfin Enhanced) may race with JavaScript Injector during startup. After Jellyfin finishes starting, run **Dashboard → Scheduled Tasks → JavaScript Injector Startup** manually, then force-reload the client. The injector block should then remain in `index.html`.
- **Daemon unavailable:** confirm the socket path, shared token, service status, and that Jellyfin can access the `jellyfin` group-owned socket.
- **No detections:** ensure `profanity-words.json` is installed (from better-profane-words) or set `FILTERD_PROFANITY_TERMS`; verify subtitle files are readable; configure Whisper/OpenVINO separately for speech or visual analysis.
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
