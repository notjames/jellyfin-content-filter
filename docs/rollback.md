# Rollback

1. Disable the JavaScript Injector entry or remove its script URL. Reload clients and confirm the Content Filter control is absent.
2. Disable the Jellyfin Content Filter plugin through Jellyfin's supported plugin workflow.
3. Stop and disable `jellyfin-content-filter.service`.
4. Run `packaging/scripts/uninstall.sh` as root, or use the same package manager that installed the files.

The uninstall script removes only program-managed files. It preserves `/etc/jellyfin-content-filter` and `/var/lib/jellyfin-content-filter` to avoid data loss. Archive or remove those directories separately only after deciding they are no longer needed.

To roll back to a prior release, disable the service, restore a matching service binary and plugin version, review configuration compatibility, and then restart. Browser bundles and plugin APIs should be rolled back together. Playback remains available if the filter is offline because the browser client fails open.
