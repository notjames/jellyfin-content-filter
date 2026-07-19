using MediaBrowser.Model.Plugins;

namespace Jellyfin.Plugin.ContentFilter.Configuration;

/// <summary>Persistent plugin settings. Jellyfin stores this as plugin XML.</summary>
public sealed class PluginConfiguration : BasePluginConfiguration
{
    /// <summary>Gets or sets the filter daemon Unix socket.</summary>
    public string SocketPath { get; set; } = "/run/jellyfin-content-filter/filterd.sock";

    /// <summary>Gets or sets the shared bearer token sent only over the Unix socket.</summary>
    public string DaemonToken { get; set; } = string.Empty;

    /// <summary>Gets or sets the daemon request timeout in seconds.</summary>
    public int TimeoutSeconds { get; set; } = 3;

    /// <summary>Gets or sets whether playback-facing lookups fail open.</summary>
    public bool FailOpen { get; set; } = true;

    /// <summary>Gets or sets whether the scheduled library analysis task is enabled.</summary>
    public bool EnableScheduledAnalysis { get; set; } = true;
}
