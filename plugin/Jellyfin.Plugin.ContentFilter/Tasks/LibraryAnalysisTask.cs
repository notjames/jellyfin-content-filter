using Jellyfin.Data.Enums;
using Jellyfin.Plugin.ContentFilter.Models;
using Jellyfin.Plugin.ContentFilter.Services;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Library;
using MediaBrowser.Model.Tasks;
using Microsoft.Extensions.Logging;

namespace Jellyfin.Plugin.ContentFilter.Tasks;

/// <summary>Periodically asks the daemon to reconcile and analyze the Jellyfin library.</summary>
public sealed class LibraryAnalysisTask : IScheduledTask
{
    private readonly IDaemonClient _daemon;
    private readonly ILibraryManager _libraryManager;
    private readonly ILogger<LibraryAnalysisTask> _logger;

    /// <summary>Initializes a new instance of the <see cref="LibraryAnalysisTask"/> class.</summary>
    public LibraryAnalysisTask(
        IDaemonClient daemon,
        ILibraryManager libraryManager,
        ILogger<LibraryAnalysisTask> logger)
    {
        _daemon = daemon;
        _libraryManager = libraryManager;
        _logger = logger;
    }

    /// <inheritdoc />
    public string Name => "Analyze library for content filtering";

    /// <inheritdoc />
    public string Key => "ContentFilterLibraryAnalysis";

    /// <inheritdoc />
    public string Description => "Queues eligible library items for analysis by the local content-filter daemon.";

    /// <inheritdoc />
    public string Category => "Content Filter";

    /// <inheritdoc />
    public bool IsHidden => false;

    /// <inheritdoc />
    public bool IsEnabled => Plugin.Instance?.Configuration.EnableScheduledAnalysis ?? false;

    /// <inheritdoc />
    public bool IsLogged => true;

    /// <inheritdoc />
    public IEnumerable<TaskTriggerInfo> GetDefaultTriggers()
    {
        yield return new TaskTriggerInfo
        {
            Type = TaskTriggerInfoType.IntervalTrigger,
            IntervalTicks = TimeSpan.FromHours(24).Ticks,
        };
    }

    /// <inheritdoc />
    public async Task ExecuteAsync(IProgress<double> progress, CancellationToken cancellationToken)
    {
        progress.Report(0);
        if (!IsEnabled)
        {
            _logger.LogInformation("Scheduled content analysis is disabled");
            progress.Report(100);
            return;
        }

        var items = _libraryManager.GetItemList(new InternalItemsQuery
        {
            Recursive = true,
            IsVirtualItem = false,
            IncludeItemTypes = [BaseItemKind.Movie, BaseItemKind.Episode],
        }).Where(MediaEligibility.IsVideoFilterTarget).ToArray();

        for (var index = 0; index < items.Length; index++)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var item = items[index];
            await _daemon.SendAsync<AnalysisEnqueueDto>(
                HttpMethod.Post,
                "/analyze",
                new DaemonAnalyzeRequest(item.Id.ToString("N"), item.Path, Force: false),
                cancellationToken).ConfigureAwait(false);
            progress.Report(((index + 1D) / items.Length) * 100D);
        }

        progress.Report(100);
    }
}
