# Architecture

## Data path

1. Jellyfin Web requests `Items/{itemId}/PlaybackInfo`.
2. The injected browser bundle recognizes that public HTTP contract and records the request's origin, reverse-proxy base path, and authorization headers.
3. The client requests `ContentFilter/Items/{itemId}` through the Jellyfin origin. The plugin is expected to authenticate the Jellyfin user and proxy or serve viewer-scoped policy.
4. Policy is resolved in this order: interval action, title category action, viewer category action, allow.
5. The scheduler applies actions to the current HTML video element.

No Jellyfin module names, webpack objects, React components, or other minified implementation details are accessed.

## API contract

`GET ContentFilter/Items/{id}` returns:

```json
{
  "viewer": { "categories": { "violence": "skip" } },
  "title": { "categories": { "language": "mute" } },
  "intervals": [
    { "start": 12.5, "end": 20.0, "category": "violence" }
  ]
}
```

`PUT ContentFilter/Viewer` accepts the `viewer` object. `PUT ContentFilter/Items/{id}` accepts the `title` object. Actions are `allow`, `skip`, or `mute`; times are seconds.

## Scheduler behavior

DOM mutation observation handles player replacement. Every replacement restores filter-owned mute state on the old element. Playback events and a boundary timer cover coarse `timeupdate` delivery. Seeking into a skip interval is evaluated immediately. Touching or overlapping skip ranges are closed into one target. A guarded retry accommodates delayed HLS seeks without continuously fighting the player. A new episode's PlaybackInfo request invalidates stale policy responses.

Policy errors clear intervals and visibly report that playback is unfiltered.
