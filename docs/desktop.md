# Jellyfin Desktop target

The desktop target supports the current Linux `jellyfin/jellyfin-desktop` client from its `main` branch. It is intentionally separate from the Jellyfin Web target:

- `web/dist/jellyfin-content-filter.js` controls an HTML5 `<video>` element.
- `desktop/dist/jellyfin-content-filter-desktop.js` controls native mpv through Jellyfin Desktop's compatibility bridge.

Build it with:

```sh
npm ci
npm run build:desktop
```

Register `desktop/dist/jellyfin-content-filter-desktop.js` in JavaScript Injector. It may be registered alongside the Web bundle on a server used by both client types: the desktop bundle refuses to start without the native bridge, and the Web bundle refuses to start inside Jellyfin Desktop, so exactly one scheduler starts.

The adapter uses the supported compatibility surface installed by the desktop client:

- `window.api.player.positionUpdate`
- `window.api.player.getPosition()`
- `window.api.player.seekTo(milliseconds)`
- `window.api.player.setMuted(boolean)`
- `window._mpvVideoPlayerInstance.isMuted()` to preserve the viewer's mute state

The content-filter server plugin and Rust daemon are unchanged. Item detection and authenticated configuration requests still use Jellyfin's `Items/{id}/PlaybackInfo` request.

## Compatibility boundary

This target is pinned to the current desktop bridge shape and must be retested when Jellyfin Desktop changes its injected `native-shim.js` or `mpv-player-base.js`. It does not patch or rebuild Jellyfin Desktop. Legacy Qt Jellyfin Media Player exposes a similar API, but it is not the supported target of this bundle.

Filtering remains fail-open. Downloads, external players, casting, and disabled JavaScript bypass it.
