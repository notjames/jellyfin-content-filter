using System.Globalization;
using Jellyfin.Plugin.ContentFilter.Configuration;
using Jellyfin.Plugin.ContentFilter.Models;
using Jellyfin.Plugin.ContentFilter.Services;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Library;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Mvc;

namespace Jellyfin.Plugin.ContentFilter.Controllers;

/// <summary>Authenticated bridge between Jellyfin clients and the local filter daemon.</summary>
[ApiController]
[Authorize]
[Route("ContentFilter")]
[Produces("application/json")]
public sealed class ContentFilterController : ControllerBase
{
    private const string AdministratorRole = "Administrator";
    private const string JellyfinUserIdClaim = "Jellyfin-UserId";
    private readonly IDaemonClient _daemon;
    private readonly ILibraryManager _libraryManager;

    /// <summary>Initializes a new instance of the <see cref="ContentFilterController"/> class.</summary>
    public ContentFilterController(IDaemonClient daemon, ILibraryManager libraryManager)
    {
        _daemon = daemon;
        _libraryManager = libraryManager;
    }

    /// <summary>Checks connectivity to the local daemon.</summary>
    [HttpGet("health")]
    [ProducesResponseType<HealthDto>(StatusCodes.Status200OK)]
    [ProducesResponseType(StatusCodes.Status503ServiceUnavailable)]
    public Task<ActionResult<HealthDto>> GetHealth(CancellationToken cancellationToken)
        => SendAsync<HealthDto>(HttpMethod.Get, "/health", null, cancellationToken);

    /// <summary>Gets the exact playback configuration expected by the browser client.</summary>
    [HttpGet("Items/{itemId:guid}")]
    [ProducesResponseType<BrowserFilterConfigDto>(StatusCodes.Status200OK)]
    [ProducesResponseType(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<BrowserFilterConfigDto>> GetItemFilterConfiguration(
        Guid itemId,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        if (_libraryManager.GetItemById<BaseItem>(itemId, userId) is null)
        {
            return NotFound();
        }

        return await SendAsync<BrowserFilterConfigDto>(
            HttpMethod.Get,
            $"/config/{itemId:N}/{userId:N}",
            null,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Updates category defaults for the authenticated viewer.</summary>
    [HttpPut("Viewer")]
    public async Task<IActionResult> PutViewerConfiguration(
        [FromBody] CategoryActionsDto request,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        return await SendWithoutResponseAsync(
            HttpMethod.Put,
            $"/viewer/{userId:N}",
            request,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Updates title-specific category actions for a visible item.</summary>
    [HttpPut("Items/{itemId:guid}")]
    public async Task<IActionResult> PutItemFilterConfiguration(
        Guid itemId,
        [FromBody] CategoryActionsDto request,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        if (_libraryManager.GetItemById<BaseItem>(itemId, userId) is null)
        {
            return NotFound();
        }

        return await SendWithoutResponseAsync(
            HttpMethod.Put,
            $"/title/{userId:N}/{itemId:N}",
            request,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Gets the effective filtered timeline for a visible library item.</summary>
    [HttpGet("timeline/{itemId:guid}")]
    [ProducesResponseType<TimelineDto>(StatusCodes.Status200OK)]
    [ProducesResponseType(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<TimelineDto>> GetTimeline(
        Guid itemId,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        if (_libraryManager.GetItemById<BaseItem>(itemId, userId) is null)
        {
            // Deliberately do not distinguish a hidden item from a missing item.
            return NotFound();
        }

        try
        {
            var timeline = await _daemon.SendAsync<DaemonTimelineDto>(
                HttpMethod.Get,
                $"/timeline/{itemId:N}/{userId:N}",
                null,
                cancellationToken).ConfigureAwait(false);
            return new TimelineDto(
                itemId,
                userId,
                timeline.Intervals,
                timeline.Revision,
                timeline.MediaFingerprint);
        }
        catch (DaemonUnavailableException ex) when (Plugin.Instance?.Configuration.FailOpen == true)
        {
            return new TimelineDto(
                itemId,
                userId,
                Array.Empty<TimelineSegmentDto>(),
                FailOpen: true,
                Warning: ex.Message);
        }
        catch (DaemonUnavailableException ex)
        {
            return DaemonUnavailable<TimelineDto>(ex);
        }
        catch (DaemonResponseException ex)
        {
            return DaemonError<TimelineDto>(ex);
        }
    }

    /// <summary>Gets the current user's policy.</summary>
    [HttpGet("policies")]
    public async Task<ActionResult<PolicyDto>> GetPolicy(CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        return await SendAsync<PolicyDto>(
            HttpMethod.Get,
            $"/policies/{userId:N}",
            null,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Updates the current user's policy.</summary>
    [HttpPut("policies")]
    public async Task<ActionResult<PolicyDto>> PutPolicy(
        [FromBody] PolicyDto policy,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        var scopedPolicy = policy with { Id = userId };
        return await SendAsync<PolicyDto>(
            HttpMethod.Put,
            $"/policies/{userId:N}",
            scopedPolicy,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Enqueues an item for administrator-requested analysis.</summary>
    [HttpPost("analysis")]
    [Authorize(Roles = AdministratorRole)]
    public async Task<ActionResult<AnalysisEnqueueDto>> EnqueueAnalysis(
        [FromBody] EnqueueAnalysisRequest request,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        var item = _libraryManager.GetItemById<BaseItem>(request.ItemId);
        if (item is null)
        {
            return NotFound();
        }

        if (string.IsNullOrWhiteSpace(item.Path))
        {
            return BadRequest("The selected item has no local media path.");
        }

        return await SendAsync<AnalysisEnqueueDto>(
            HttpMethod.Post,
            "/analyze",
            new DaemonAnalyzeRequest(
                request.ItemId.ToString("N", CultureInfo.InvariantCulture),
                item.Path,
                request.Force),
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Gets an analysis job's status.</summary>
    [HttpGet("analysis/{jobId:guid}")]
    [Authorize(Roles = AdministratorRole)]
    public Task<ActionResult<AnalysisStatusDto>> GetAnalysisStatus(
        Guid jobId,
        CancellationToken cancellationToken)
        => SendAsync<AnalysisStatusDto>(
            HttpMethod.Get,
            $"/analyze/{jobId:N}",
            null,
            cancellationToken);

    /// <summary>Lists recent analysis jobs and per-status counts.</summary>
    [HttpGet("jobs")]
    [Authorize(Roles = AdministratorRole)]
    public Task<ActionResult<AnalysisJobsSummaryDto>> GetAnalysisJobs(
        [FromQuery] string? status,
        [FromQuery] int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var path = $"/jobs?limit={Math.Clamp(limit, 1, 500)}";
        if (!string.IsNullOrWhiteSpace(status))
        {
            path += $"&status={Uri.EscapeDataString(status)}";
        }

        return SendAsync<AnalysisJobsSummaryDto>(HttpMethod.Get, path, null, cancellationToken);
    }

    /// <summary>Lists analysis problems for administrator investigation.</summary>
    [HttpGet("problems")]
    [Authorize(Roles = AdministratorRole)]
    public Task<ActionResult<IReadOnlyList<MediaAnalysisStateDto>>> GetAnalysisProblems(
        CancellationToken cancellationToken)
        => SendAsync<IReadOnlyList<MediaAnalysisStateDto>>(
            HttpMethod.Get,
            "/media/problems",
            null,
            cancellationToken);

    /// <summary>Lists machine-generated candidates for a visible item.</summary>
    [HttpGet("review/{itemId:guid}")]
    [Authorize(Roles = AdministratorRole)]
    public async Task<ActionResult<IReadOnlyList<ReviewCandidateDto>>> GetReviewCandidates(
        Guid itemId,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        return await SendAsync<IReadOnlyList<ReviewCandidateDto>>(
            HttpMethod.Get,
            $"/media/{itemId:N}/candidates",
            null,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Lists analysis evidence for a visible item.</summary>
    [HttpGet("review/{itemId:guid}/evidence")]
    [Authorize(Roles = AdministratorRole)]
    public async Task<ActionResult<IReadOnlyList<EvidenceDto>>> GetReviewEvidence(
        Guid itemId,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Unauthorized();
        }

        return await SendAsync<IReadOnlyList<EvidenceDto>>(
            HttpMethod.Get,
            $"/media/{itemId:N}/evidence",
            null,
            cancellationToken).ConfigureAwait(false);
    }

    /// <summary>Records an administrator's candidate decision.</summary>
    [HttpPost("review/{itemId:guid}/{candidateId:guid}")]
    [Authorize(Roles = AdministratorRole)]
    public Task<ActionResult<ReviewCandidateDto>> ReviewCandidate(
        Guid itemId,
        Guid candidateId,
        [FromBody] ReviewDecisionRequest request,
        CancellationToken cancellationToken)
    {
        if (!TryGetCurrentUserId(out var userId))
        {
            return Task.FromResult<ActionResult<ReviewCandidateDto>>(Unauthorized());
        }

        var decision = request.Decision.Trim().ToLowerInvariant();
        if (decision is not ("approve" or "reject" or "edit")
            || (decision == "edit" && request.Edit is null))
        {
            return Task.FromResult<ActionResult<ReviewCandidateDto>>(
                BadRequest("Decision must be approve, reject, or edit; edit requires an Edit value."));
        }

        return SendAsync<ReviewCandidateDto>(
            HttpMethod.Post,
            $"/candidates/{candidateId:N}/{decision}",
            new DaemonReviewRequest(userId.ToString("N", CultureInfo.InvariantCulture), request.Edit),
            cancellationToken);
    }

    /// <summary>Gets non-secret plugin configuration.</summary>
    [HttpGet("configuration")]
    [Authorize(Roles = AdministratorRole)]
    public ActionResult<ConfigurationDto> GetConfiguration()
    {
        var configuration = Plugin.Instance?.Configuration;
        if (configuration is null)
        {
            return Problem(statusCode: StatusCodes.Status503ServiceUnavailable);
        }

        return new ConfigurationDto(
            configuration.SocketPath,
            !string.IsNullOrWhiteSpace(configuration.DaemonToken),
            configuration.TimeoutSeconds,
            configuration.FailOpen,
            configuration.EnableScheduledAnalysis);
    }

    /// <summary>Updates plugin configuration.</summary>
    [HttpPut("configuration")]
    [Authorize(Roles = AdministratorRole)]
    public IActionResult PutConfiguration([FromBody] UpdateConfigurationRequest request)
    {
        var plugin = Plugin.Instance;
        if (plugin is null)
        {
            return Problem(statusCode: StatusCodes.Status503ServiceUnavailable);
        }

        if (!Path.IsPathFullyQualified(request.SocketPath)
            || request.TimeoutSeconds is < 1 or > 120)
        {
            return ValidationProblem("SocketPath must be absolute and TimeoutSeconds must be between 1 and 120.");
        }

        var old = plugin.Configuration;
        plugin.UpdateConfiguration(new PluginConfiguration
        {
            SocketPath = request.SocketPath,
            DaemonToken = request.DaemonToken ?? old.DaemonToken,
            TimeoutSeconds = request.TimeoutSeconds,
            FailOpen = request.FailOpen,
            EnableScheduledAnalysis = request.EnableScheduledAnalysis,
        });
        return Ok(new { Status = "OK" });
    }

    private bool TryGetCurrentUserId(out Guid userId)
    {
        var value = User.FindFirst(JellyfinUserIdClaim)?.Value;
        return Guid.TryParseExact(value, "N", out userId)
            || Guid.TryParse(value, CultureInfo.InvariantCulture, out userId);
    }

    private async Task<ActionResult<T>> SendAsync<T>(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken)
    {
        try
        {
            return await _daemon.SendAsync<T>(method, path, body, cancellationToken).ConfigureAwait(false);
        }
        catch (DaemonUnavailableException ex)
        {
            return DaemonUnavailable<T>(ex);
        }
        catch (DaemonResponseException ex)
        {
            return DaemonError<T>(ex);
        }
    }

    private async Task<IActionResult> SendWithoutResponseAsync(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken)
    {
        try
        {
            await _daemon.SendAsync(method, path, body, cancellationToken).ConfigureAwait(false);
            // The browser wrapper always parses successful responses as JSON.
            return Ok(new { });
        }
        catch (DaemonUnavailableException ex)
        {
            return DaemonUnavailable<object>(ex).Result!;
        }
        catch (DaemonResponseException ex)
        {
            return DaemonError<object>(ex).Result!;
        }
    }

    private ActionResult<T> DaemonUnavailable<T>(DaemonUnavailableException exception)
        => Problem(
            detail: exception.Message,
            statusCode: StatusCodes.Status503ServiceUnavailable,
            title: "Content filter daemon unavailable");

    private ActionResult<T> DaemonError<T>(DaemonResponseException exception)
    {
        var status = exception.StatusCode switch
        {
            System.Net.HttpStatusCode.BadRequest => StatusCodes.Status400BadRequest,
            System.Net.HttpStatusCode.UnprocessableEntity => StatusCodes.Status400BadRequest,
            System.Net.HttpStatusCode.NotFound => StatusCodes.Status404NotFound,
            System.Net.HttpStatusCode.Conflict => StatusCodes.Status409Conflict,
            System.Net.HttpStatusCode.TooManyRequests => StatusCodes.Status429TooManyRequests,
            System.Net.HttpStatusCode.Unauthorized => StatusCodes.Status502BadGateway,
            System.Net.HttpStatusCode.Forbidden => StatusCodes.Status502BadGateway,
            >= System.Net.HttpStatusCode.InternalServerError => StatusCodes.Status503ServiceUnavailable,
            _ => StatusCodes.Status502BadGateway,
        };
        return Problem(
            detail: exception.Message,
            statusCode: status,
            title: "Content filter daemon rejected the request");
    }
}
