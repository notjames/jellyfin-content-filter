using System.Text.Json;
using System.Text.Json.Serialization;

namespace Jellyfin.Plugin.ContentFilter.Models;

/// <summary>Daemon health information.</summary>
public sealed record HealthDto(string Status, string? Version = null);

/// <summary>Per-category playback actions used by the browser client.</summary>
public sealed record CategoryActionsDto(
    [property: JsonPropertyName("categories")] IReadOnlyDictionary<string, string> Categories);

/// <summary>A browser playback interval expressed in seconds.</summary>
public sealed record BrowserIntervalDto(
    [property: JsonPropertyName("start")] double Start,
    [property: JsonPropertyName("end")] double End,
    [property: JsonPropertyName("category")] string Category,
    [property: JsonPropertyName("action")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    string? Action = null);

/// <summary>Exact playback configuration consumed by the injected browser client.</summary>
public sealed record BrowserFilterConfigDto(
    [property: JsonPropertyName("viewer")] CategoryActionsDto Viewer,
    [property: JsonPropertyName("title")] CategoryActionsDto Title,
    [property: JsonPropertyName("intervals")] IReadOnlyList<BrowserIntervalDto> Intervals,
    [property: JsonPropertyName("filter_status")] string FilterStatus = "missing",
    [property: JsonPropertyName("problems")] IReadOnlyList<FilterProblemDto>? Problems = null,
    [property: JsonPropertyName("filters_enabled")] bool FiltersEnabled = true,
    [property: JsonPropertyName("warning")] string? Warning = null);

/// <summary>A stored analysis problem for administrator investigation.</summary>
public sealed record FilterProblemDto(
    [property: JsonPropertyName("code")] string Code,
    [property: JsonPropertyName("message")] string Message);

/// <summary>Persisted analysis status for a library item.</summary>
public sealed record MediaAnalysisStateDto(
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("media_path")] string MediaPath,
    [property: JsonPropertyName("fingerprint")] string Fingerprint,
    [property: JsonPropertyName("status")] string Status,
    [property: JsonPropertyName("problems")] IReadOnlyList<FilterProblemDto> Problems,
    [property: JsonPropertyName("analyzed_at_ms")] long AnalyzedAtMilliseconds);

/// <summary>A time range carrying content classifications.</summary>
public sealed record TimelineSegmentDto(
    [property: JsonPropertyName("start_ms")] ulong StartMilliseconds,
    [property: JsonPropertyName("end_ms")] ulong EndMilliseconds,
    [property: JsonPropertyName("action")] string Action,
    [property: JsonPropertyName("category")] string Category,
    [property: JsonPropertyName("severity")] string Severity);

/// <summary>A user's effective timeline for an item.</summary>
public sealed record TimelineDto(
    Guid ItemId,
    Guid UserId,
    IReadOnlyList<TimelineSegmentDto> Segments,
    ulong Revision = 0,
    string? MediaFingerprint = null,
    bool FailOpen = false,
    string? Warning = null);

/// <summary>A user policy rule.</summary>
public sealed record PolicyRuleDto(
    [property: JsonPropertyName("category")] string Category,
    [property: JsonPropertyName("minimum_severity")] string MinimumSeverity,
    [property: JsonPropertyName("action")] string Action,
    [property: JsonPropertyName("enabled")] bool Enabled = true);

/// <summary>A user's policy document.</summary>
public sealed record PolicyDto(
    [property: JsonPropertyName("id")] Guid Id,
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("rules")] IReadOnlyList<PolicyRuleDto> Rules);

/// <summary>Request to enqueue one item for analysis.</summary>
public sealed record EnqueueAnalysisRequest(Guid ItemId, bool Force = false);

/// <summary>Response from an analysis enqueue operation.</summary>
public sealed record AnalysisEnqueueDto(
    [property: JsonPropertyName("id")] Guid JobId,
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("media_path")] string MediaPath,
    [property: JsonPropertyName("status")] string Status,
    [property: JsonPropertyName("error")] string? Error,
    [property: JsonPropertyName("created_at_ms")] long CreatedAtMilliseconds,
    [property: JsonPropertyName("updated_at_ms")] long UpdatedAtMilliseconds);

/// <summary>Current analysis job state.</summary>
public sealed record AnalysisStatusDto(
    [property: JsonPropertyName("id")] Guid JobId,
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("media_path")] string MediaPath,
    [property: JsonPropertyName("status")] string Status,
    [property: JsonPropertyName("error")] string? Error,
    [property: JsonPropertyName("created_at_ms")] long CreatedAtMilliseconds,
    [property: JsonPropertyName("updated_at_ms")] long UpdatedAtMilliseconds);

/// <summary>A machine-generated candidate awaiting administrator review.</summary>
public sealed record ReviewCandidateDto(
    [property: JsonPropertyName("id")] Guid CandidateId,
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("interval")] TimelineSegmentDto Interval,
    [property: JsonPropertyName("evidence_ids")] IReadOnlyList<Guid> EvidenceIds,
    [property: JsonPropertyName("status")] string Status);

/// <summary>Analysis evidence associated with a review candidate.</summary>
public sealed record EvidenceDto(
    [property: JsonPropertyName("id")] Guid Id,
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("start_ms")] ulong StartMilliseconds,
    [property: JsonPropertyName("end_ms")] ulong EndMilliseconds,
    [property: JsonPropertyName("category")] string Category,
    [property: JsonPropertyName("severity")] string Severity,
    [property: JsonPropertyName("confidence")] double? Confidence,
    [property: JsonPropertyName("source")] string Source,
    [property: JsonPropertyName("details")] JsonElement Details,
    [property: JsonPropertyName("raw_score")]
    [property: JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    double? RawScore = null);

/// <summary>Administrator decision for a review candidate.</summary>
public sealed record ReviewDecisionRequest(string Decision, ReviewEditDto? Edit = null);

/// <summary>Replacement interval supplied for an edit decision.</summary>
public sealed record ReviewEditDto(
    [property: JsonPropertyName("start_ms")] ulong StartMilliseconds,
    [property: JsonPropertyName("end_ms")] ulong EndMilliseconds,
    [property: JsonPropertyName("action")] string Action,
    [property: JsonPropertyName("category")] string Category,
    [property: JsonPropertyName("severity")] string Severity);

/// <summary>Plugin configuration returned by the protected API.</summary>
public sealed record ConfigurationDto(
    string SocketPath,
    bool TokenConfigured,
    int TimeoutSeconds,
    bool FailOpen,
    bool EnableScheduledAnalysis);

/// <summary>Plugin configuration update. A null token preserves the current secret.</summary>
public sealed record UpdateConfigurationRequest(
    string SocketPath,
    string? DaemonToken,
    int TimeoutSeconds,
    bool FailOpen,
    bool EnableScheduledAnalysis);

/// <summary>Opaque JSON response used for forward-compatible daemon operations.</summary>
public sealed class DaemonDocumentDto
{
    /// <summary>Gets additional daemon fields.</summary>
    [JsonExtensionData]
    public IDictionary<string, JsonElement> Values { get; init; } = new Dictionary<string, JsonElement>();
}

internal sealed record DaemonAnalyzeRequest(
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("media_path")] string MediaPath,
    [property: JsonPropertyName("force")] bool Force = false);

/// <summary>Aggregate analysis job counts and recent jobs.</summary>
public sealed record AnalysisJobsSummaryDto(
    [property: JsonPropertyName("counts")] IReadOnlyDictionary<string, long> Counts,
    [property: JsonPropertyName("jobs")] IReadOnlyList<AnalysisStatusDto> Jobs);

internal sealed record DaemonReviewRequest(
    [property: JsonPropertyName("reviewer")] string Reviewer,
    [property: JsonPropertyName("edit")] ReviewEditDto? Edit = null);

internal sealed record DaemonTimelineDto(
    [property: JsonPropertyName("media_id")] string MediaId,
    [property: JsonPropertyName("policy_id")] Guid PolicyId,
    [property: JsonPropertyName("media_fingerprint")] string MediaFingerprint,
    [property: JsonPropertyName("revision")] ulong Revision,
    [property: JsonPropertyName("intervals")] IReadOnlyList<TimelineSegmentDto> Intervals);
