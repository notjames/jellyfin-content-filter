using System.Reflection;
using System.Text.Json;
using Jellyfin.Plugin.ContentFilter.Configuration;
using Jellyfin.Plugin.ContentFilter.Controllers;
using Jellyfin.Plugin.ContentFilter.Models;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;
using Microsoft.AspNetCore.Mvc.Routing;
using Xunit;

namespace Jellyfin.Plugin.ContentFilter.Tests;

public sealed class ContractTests
{
    [Fact]
    public void ConfigurationDefaultsAreSafeAndLocal()
    {
        var configuration = new PluginConfiguration();

        Assert.StartsWith("/", configuration.SocketPath, StringComparison.Ordinal);
        Assert.Empty(configuration.DaemonToken);
        Assert.InRange(configuration.TimeoutSeconds, 1, 120);
        Assert.True(configuration.FailOpen);
    }

    [Theory]
    [InlineData(nameof(ContentFilterController.EnqueueAnalysis))]
    [InlineData(nameof(ContentFilterController.GetAnalysisStatus))]
    [InlineData(nameof(ContentFilterController.GetAnalysisJobs))]
    [InlineData(nameof(ContentFilterController.GetAnalysisProblems))]
    [InlineData(nameof(ContentFilterController.GetReviewCandidates))]
    [InlineData(nameof(ContentFilterController.GetReviewEvidence))]
    [InlineData(nameof(ContentFilterController.ReviewCandidate))]
    [InlineData(nameof(ContentFilterController.GetConfiguration))]
    [InlineData(nameof(ContentFilterController.PutConfiguration))]
    public void SensitiveEndpointsRequireAdministratorRole(string methodName)
    {
        var methods = typeof(ContentFilterController)
            .GetMethods(BindingFlags.Instance | BindingFlags.Public)
            .Where(method => method.Name == methodName);

        Assert.All(methods, method =>
        {
            var authorization = method.GetCustomAttribute<AuthorizeAttribute>();
            Assert.NotNull(authorization);
            Assert.Equal("Administrator", authorization!.Roles);
        });
    }

    [Fact]
    public void ControllerRequiresAuthenticationByDefault()
    {
        var authorization = typeof(ContentFilterController).GetCustomAttribute<AuthorizeAttribute>();
        Assert.NotNull(authorization);
        Assert.Null(authorization!.Roles);
    }

    [Fact]
    public void BrowserConfigurationSerializesToExactClientContract()
    {
        var configuration = new BrowserFilterConfigDto(
            new CategoryActionsDto(new Dictionary<string, string> { ["violence"] = "mute" }),
            new CategoryActionsDto(new Dictionary<string, string> { ["language"] = "skip" }),
            new[]
            {
                new BrowserIntervalDto(1.25, 2.5, "violence"),
                new BrowserIntervalDto(3, 4, "language", "mute"),
            });

        var json = JsonSerializer.Serialize(configuration);

        Assert.Equal(
            "{\"viewer\":{\"categories\":{\"violence\":\"mute\"}},"
            + "\"title\":{\"categories\":{\"language\":\"skip\"}},"
            + "\"intervals\":[{\"start\":1.25,\"end\":2.5,\"category\":\"violence\"},"
            + "{\"start\":3,\"end\":4,\"category\":\"language\",\"action\":\"mute\"}],"
            + "\"filter_status\":\"missing\",\"problems\":null,\"filters_enabled\":true,\"warning\":null}",
            json);
    }

    [Fact]
    public void CategoryUpdateDeserializesFromBrowserContract()
    {
        var request = JsonSerializer.Deserialize<CategoryActionsDto>(
            "{\"categories\":{\"violence\":\"skip\",\"language\":\"allow\"}}");

        Assert.NotNull(request);
        Assert.Equal("skip", request!.Categories["violence"]);
        Assert.Equal("allow", request.Categories["language"]);
    }

    [Fact]
    public void EvidenceSerializesWithDaemonFieldNamesAndJsonDetails()
    {
        var details = JsonSerializer.Deserialize<JsonElement>(
            "{\"transcript\":\"untrusted <script>\",\"model\":\"whisper\"}");
        var evidence = new EvidenceDto(
            Guid.Parse("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"),
            "media",
            1000,
            2500,
            "language",
            "medium",
            0.875,
            "transcript",
            details,
            -1.25);

        var json = JsonSerializer.Serialize(evidence);
        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;

        Assert.Equal(1000UL, root.GetProperty("start_ms").GetUInt64());
        Assert.Equal(2500UL, root.GetProperty("end_ms").GetUInt64());
        Assert.Equal(0.875, root.GetProperty("confidence").GetDouble());
        Assert.Equal(-1.25, root.GetProperty("raw_score").GetDouble());
        Assert.Equal("untrusted <script>", root.GetProperty("details").GetProperty("transcript").GetString());
        Assert.Equal("whisper", root.GetProperty("details").GetProperty("model").GetString());
    }

    [Fact]
    public void ReviewEvidenceRouteIsItemScoped()
    {
        var method = typeof(ContentFilterController).GetMethod(
            nameof(ContentFilterController.GetReviewEvidence));
        var route = method!.GetCustomAttribute<HttpGetAttribute>();

        Assert.NotNull(route);
        Assert.Equal("review/{itemId:guid}/evidence", route!.Template);
    }

    [Theory]
    [InlineData(nameof(ContentFilterController.GetItemFilterConfiguration), "Items/{itemId:guid}", "GET")]
    [InlineData(nameof(ContentFilterController.PutViewerConfiguration), "Viewer", "PUT")]
    [InlineData(nameof(ContentFilterController.PutItemFilterConfiguration), "Items/{itemId:guid}", "PUT")]
    public void BrowserRoutesMatchInjectedClientContract(string methodName, string template, string verb)
    {
        var method = typeof(ContentFilterController).GetMethod(methodName);

        Assert.NotNull(method);
        var route = method!.GetCustomAttributes<HttpMethodAttribute>().Single();
        Assert.Equal(template, route.Template);
        Assert.Contains(verb, route.HttpMethods);
        Assert.Null(method.GetCustomAttribute<AuthorizeAttribute>());
    }
}
