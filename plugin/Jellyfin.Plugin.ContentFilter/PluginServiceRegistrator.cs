using Jellyfin.Plugin.ContentFilter.Services;
using Jellyfin.Plugin.ContentFilter.Tasks;
using MediaBrowser.Controller;
using MediaBrowser.Controller.Plugins;
using MediaBrowser.Model.Tasks;
using Microsoft.Extensions.DependencyInjection;

namespace Jellyfin.Plugin.ContentFilter;

/// <summary>Registers plugin services with Jellyfin's host container.</summary>
public sealed class PluginServiceRegistrator : IPluginServiceRegistrator
{
    /// <inheritdoc />
    public void RegisterServices(IServiceCollection serviceCollection, IServerApplicationHost applicationHost)
    {
        serviceCollection.AddSingleton<IDaemonClient, DaemonClient>();
        serviceCollection.AddSingleton<IScheduledTask, LibraryAnalysisTask>();
        serviceCollection.AddHostedService<LibraryMonitorHostedService>();
    }
}
