using Jellyfin.Data.Enums;
using Jellyfin.Plugin.ContentFilter.Models;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Library;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace Jellyfin.Plugin.ContentFilter.Services;

/// <summary>Queues analysis when movies or episodes are added to the library.</summary>
public sealed class LibraryMonitorHostedService : IHostedService
{
    private readonly ILibraryManager _libraryManager;
    private readonly IDaemonClient _daemon;
    private readonly ILogger<LibraryMonitorHostedService> _logger;

    /// <summary>Initializes a new instance of the <see cref="LibraryMonitorHostedService"/> class.</summary>
    public LibraryMonitorHostedService(
        ILibraryManager libraryManager,
        IDaemonClient daemon,
        ILogger<LibraryMonitorHostedService> logger)
    {
        _libraryManager = libraryManager;
        _daemon = daemon;
        _logger = logger;
    }

    /// <inheritdoc />
    public Task StartAsync(CancellationToken cancellationToken)
    {
        _libraryManager.ItemAdded += OnItemAdded;
        return Task.CompletedTask;
    }

    /// <inheritdoc />
    public Task StopAsync(CancellationToken cancellationToken)
    {
        _libraryManager.ItemAdded -= OnItemAdded;
        return Task.CompletedTask;
    }

    private void OnItemAdded(object? sender, ItemChangeEventArgs args)
    {
        var item = args.Item;
        if (item is null || item.IsFolder || item.IsVirtualItem || string.IsNullOrWhiteSpace(item.Path))
        {
            return;
        }

        if (item.GetBaseItemKind() is not (BaseItemKind.Movie or BaseItemKind.Episode))
        {
            return;
        }

        _ = EnqueueAsync(item);
    }

    private async Task EnqueueAsync(BaseItem item)
    {
        try
        {
            await _daemon.SendAsync<AnalysisEnqueueDto>(
                HttpMethod.Post,
                "/analyze",
                new DaemonAnalyzeRequest(item.Id.ToString("N"), item.Path, Force: false),
                CancellationToken.None).ConfigureAwait(false);
            _logger.LogInformation("Queued content-filter analysis for {Name} ({Id})", item.Name, item.Id);
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Failed to queue content-filter analysis for {Id}", item.Id);
        }
    }
}
