# Jellyfin Content Filter

Jellyfin Content Filter skips or mutes category-tagged playback intervals according to viewer defaults and per-title overrides. The browser client is designed for Jellyfin Web's JavaScript Injector and does not depend on minified Jellyfin internals.

It consists of:

- a Rust daemon that analyzes media, stores review evidence and policies, and publishes viewer-scoped timelines over a Unix socket;
- a thin .NET 9 plugin for Jellyfin 10.11.x authentication, library access, administration, and scheduled analysis;
- a TypeScript controller for the Jellyfin Web HTML5 player.

## Build and test

Requirements: Rust 1.85+, .NET 9, Node.js 20+, npm, FFmpeg, and zip.

```sh
cargo test --workspace
cargo build --release
npm ci
npm test
npm run lint
npm run build:all
npm run test:e2e
dotnet test plugin/Jellyfin.Plugin.ContentFilter.Tests/Jellyfin.Plugin.ContentFilter.Tests.csproj
```

Install a Playwright browser once before E2E testing:

```sh
npx playwright install chromium
```

## How it works

The daemon extracts sidecar subtitles, optionally supplements them with whisper.cpp transcription, and can call the bundled `openvino-runner` on sampled frames (Intel `GPU` with `CPU` fallback) when `FILTERD_OPENVINO_RUNNER` and `FILTERD_OPENVINO_MODEL` are set. Model weights are not bundled; IR models need `.xml` plus sibling `.bin`, or a single `.onnx`. Map class scores with an optional labels JSON (`FILTERD_OPENVINO_LABELS`).

The clients observe standard `fetch` and `XMLHttpRequest` calls to Jellyfin's `Items/{id}/PlaybackInfo` endpoint. That supplies the item ID, authentication headers, origin, and any reverse-proxy base path. The Web target observes `<video>` elements; the separate Linux Jellyfin Desktop target controls native mpv through `window.api.player`. Both fail open and display a warning if policy cannot be loaded.

Policy precedence is interval override, title category override, viewer category default, then allow. Skip takes priority over mute for overlapping active intervals.

## Installation and packaging

See [INSTALL.md](INSTALL.md) for complete Jellyfin server, Jellyfin Web, and Linux Jellyfin Desktop installation instructions. Packaging assets include systemd hardening, sysusers/tmpfiles declarations, a configuration example, staging-safe install/uninstall scripts, a native archive script, and a Jellyfin catalog manifest template.

Additional documentation:

- [Architecture](docs/architecture.md)
- [Linux Jellyfin Desktop target](docs/desktop.md)
- [Security model](docs/security.md)
- [Model and third-party licensing](docs/model-licensing.md)
- [Rollback](docs/rollback.md)

## Scope and safety

Version 0.1 supports only local playback in the server-provided Jellyfin Web HTML5 player. Native TV/mobile clients, Chromecast, SyncPlay, downloads, and external players bypass it. Browser-side filtering is not tamper-resistant parental control: the original stream still reaches the browser, and analysis can miss or misclassify content. Review candidates before publishing them.

## License

Project-authored code and documentation are available under the [MIT License](LICENSE). Models, media, Jellyfin, and other third-party components retain their own licenses.
