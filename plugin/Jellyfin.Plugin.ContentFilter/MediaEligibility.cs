using Jellyfin.Data.Enums;
using MediaBrowser.Controller.Entities;

namespace Jellyfin.Plugin.ContentFilter;

/// <summary>Shared rules for which library items participate in content filtering.</summary>
public static class MediaEligibility
{
    /// <summary>
    /// Returns true when the item is local video content eligible for analysis and playback filtering.
    /// Music and other audio-only items are excluded.
    /// </summary>
    public static bool IsVideoFilterTarget(BaseItem? item)
    {
        if (item is null
            || item.IsFolder
            || item.IsVirtualItem
            || string.IsNullOrWhiteSpace(item.Path))
        {
            return false;
        }

        return item.GetBaseItemKind() is BaseItemKind.Movie or BaseItemKind.Episode;
    }
}
