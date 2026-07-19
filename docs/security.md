# Security model

Content labels and viewer preferences may reveal sensitive health, religious, political, sexual, or household information. Treat them as private user data.

## Trust boundaries

- The browser client runs with Jellyfin Web's origin privileges. Only serve the bundle from a trusted, integrity-controlled location.
- The plugin must derive viewer identity from Jellyfin authentication. It must not trust a user ID supplied by the browser.
- The native service listens on a group-restricted Unix socket by default. Its shared secret is for plugin-to-service authentication and must not be returned to browsers.
- Configuration and state should remain readable only by their service principals.

The client forwards only authentication headers already used by the PlaybackInfo request, uses same-origin credentials, and preserves the observed reverse-proxy path. It never stores tokens. Content Filter endpoints must enforce authorization and prevent one viewer from reading or changing another viewer's policy.

## Failure behavior

Filtering is not a safety boundary. Network errors, malformed policy, unsupported players, script blockers, transcoding behavior, and browser restrictions can prevent actions. The client deliberately fails open so filtering cannot make playback unavailable, and presents a visible warning. Do not represent it as parental-control enforcement or guaranteed content removal.

Validate interval bounds and category/action enums on the server. Apply request size limits, CSRF protections appropriate to Jellyfin's authentication scheme, audit sensitive changes without logging labels or tokens unnecessarily, and use TLS outside localhost.

Report vulnerabilities privately to the maintainers; do not include real media, access tokens, secrets, or user policy in reports.
