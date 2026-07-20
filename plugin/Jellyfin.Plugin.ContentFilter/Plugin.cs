using System.Reflection;
using Jellyfin.Plugin.ContentFilter.Configuration;
using MediaBrowser.Common.Configuration;
using MediaBrowser.Common.Plugins;
using MediaBrowser.Model.Plugins;
using MediaBrowser.Model.Serialization;

namespace Jellyfin.Plugin.ContentFilter;

/// <summary>Jellyfin Content Filter plugin entry point.</summary>
public sealed class Plugin : BasePlugin<PluginConfiguration>, IHasWebPages
{
    /// <summary>Stable plugin identifier.</summary>
    public static readonly Guid PluginId = Guid.Parse("6f7805b3-ea58-4c5a-a914-c74e733ace42");

    /// <summary>Initializes a new instance of the <see cref="Plugin"/> class.</summary>
    public Plugin(IApplicationPaths applicationPaths, IXmlSerializer xmlSerializer)
        : base(applicationPaths, xmlSerializer)
    {
        Instance = this;
    }

    /// <summary>Gets the active plugin instance.</summary>
    public static Plugin? Instance { get; private set; }

    /// <inheritdoc />
    public override string Name => "Content Filter";

    /// <inheritdoc />
    public override string Description => "User-specific content policies backed by the local content-filter daemon.";

    /// <inheritdoc />
    public override Guid Id => PluginId;

    /// <inheritdoc />
    public IEnumerable<PluginPageInfo> GetPages()
    {
        var assemblyName = Assembly.GetExecutingAssembly().GetName().Name;
        yield return new PluginPageInfo
        {
            Name = "ContentFilter",
            EmbeddedResourcePath = $"{assemblyName}.Web.configPage.html",
        };
        yield return new PluginPageInfo
        {
            Name = "ContentFilterReviewV2",
            DisplayName = "Content Filter Review",
            EnableInMainMenu = true,
            MenuSection = "server",
            MenuIcon = "fact_check",
            EmbeddedResourcePath = $"{assemblyName}.Web.reviewPage.html",
        };
        yield return new PluginPageInfo
        {
            Name = "ContentFilterStatus",
            DisplayName = "Content Filter Status",
            EnableInMainMenu = true,
            MenuSection = "server",
            MenuIcon = "pending_actions",
            EmbeddedResourcePath = $"{assemblyName}.Web.statusPage.html",
        };
    }
}
