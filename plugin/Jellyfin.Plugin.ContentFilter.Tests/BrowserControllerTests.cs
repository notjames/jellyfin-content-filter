using System.Net;
using System.Security.Claims;
using Jellyfin.Plugin.ContentFilter.Controllers;
using Jellyfin.Plugin.ContentFilter.Models;
using Jellyfin.Plugin.ContentFilter.Services;
using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Entities.Audio;
using MediaBrowser.Controller.Entities.Movies;
using MediaBrowser.Controller.Library;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Mvc;
using Moq;
using Xunit;

namespace Jellyfin.Plugin.ContentFilter.Tests;

public sealed class BrowserControllerTests
{
    [Fact]
    public async Task GetItemConfigChecksVisibilityAndUsesClaimScopedDaemonPath()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var expected = EmptyConfiguration();
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        daemon.Setup(client => client.SendAsync<BrowserFilterConfigDto>(
                HttpMethod.Get,
                $"/config/{itemId:N}/{userId:N}",
                null,
                It.IsAny<CancellationToken>()))
            .ReturnsAsync(expected);
        var library = VisibleItemLibrary(itemId, userId);
        var controller = CreateController(daemon.Object, library.Object, userId);

        var result = await controller.GetItemFilterConfiguration(itemId, CancellationToken.None);

        Assert.Same(expected, result.Value);
        library.Verify(manager => manager.GetItemById<BaseItem>(itemId, userId), Times.Once);
        daemon.VerifyAll();
    }

    [Fact]
    public async Task PutViewerUsesAuthenticatedUserAndReturnsJsonBody()
    {
        var userId = Guid.NewGuid();
        var request = new CategoryActionsDto(new Dictionary<string, string> { ["violence"] = "skip" });
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        daemon.Setup(client => client.SendAsync(
                HttpMethod.Put,
                $"/viewer/{userId:N}",
                request,
                It.IsAny<CancellationToken>()))
            .Returns(Task.CompletedTask);
        var controller = CreateController(
            daemon.Object,
            Mock.Of<ILibraryManager>(),
            userId);

        var result = await controller.PutViewerConfiguration(request, CancellationToken.None);

        Assert.IsType<OkObjectResult>(result);
        daemon.VerifyAll();
    }

    [Fact]
    public async Task PutTitleRejectsInvisibleItemWithoutCallingDaemon()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        var library = new Mock<ILibraryManager>(MockBehavior.Strict);
        library.Setup(manager => manager.GetItemById<BaseItem>(itemId, userId)).Returns((BaseItem?)null);
        var controller = CreateController(daemon.Object, library.Object, userId);

        var result = await controller.PutItemFilterConfiguration(
            itemId,
            new CategoryActionsDto(new Dictionary<string, string>()),
            CancellationToken.None);

        Assert.IsType<NotFoundResult>(result);
        daemon.VerifyNoOtherCalls();
    }

    [Fact]
    public async Task PutTitleChecksVisibilityAndUsesUserAndItemPath()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var request = new CategoryActionsDto(new Dictionary<string, string> { ["language"] = "mute" });
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        daemon.Setup(client => client.SendAsync(
                HttpMethod.Put,
                $"/title/{userId:N}/{itemId:N}",
                request,
                It.IsAny<CancellationToken>()))
            .Returns(Task.CompletedTask);
        var library = VisibleItemLibrary(itemId, userId);
        var controller = CreateController(daemon.Object, library.Object, userId);

        var result = await controller.PutItemFilterConfiguration(itemId, request, CancellationToken.None);

        Assert.IsType<OkObjectResult>(result);
        library.Verify(manager => manager.GetItemById<BaseItem>(itemId, userId), Times.Once);
        daemon.VerifyAll();
    }

    [Fact]
    public async Task DaemonNotFoundIsMappedToApiNotFound()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        daemon.Setup(client => client.SendAsync<BrowserFilterConfigDto>(
                HttpMethod.Get,
                It.IsAny<string>(),
                null,
                It.IsAny<CancellationToken>()))
            .ThrowsAsync(new DaemonResponseException(HttpStatusCode.NotFound, "missing"));
        var controller = CreateController(
            daemon.Object,
            VisibleItemLibrary(itemId, userId).Object,
            userId);

        var result = await controller.GetItemFilterConfiguration(itemId, CancellationToken.None);

        var error = Assert.IsType<ObjectResult>(result.Result);
        Assert.Equal(StatusCodes.Status404NotFound, error.StatusCode);
    }

    [Fact]
    public async Task ReviewEvidenceUsesMediaEvidencePath()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var expected = Array.Empty<EvidenceDto>();
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        daemon.Setup(client => client.SendAsync<IReadOnlyList<EvidenceDto>>(
                HttpMethod.Get,
                $"/media/{itemId:N}/evidence",
                null,
                It.IsAny<CancellationToken>()))
            .ReturnsAsync(expected);
        var library = new Mock<ILibraryManager>(MockBehavior.Strict);
        var controller = CreateController(daemon.Object, library.Object, userId);

        var result = await controller.GetReviewEvidence(itemId, CancellationToken.None);

        Assert.Same(expected, result.Value);
        daemon.VerifyAll();
    }

    [Fact]
    public async Task GetItemConfigReturnsInactiveConfigForAudioWithoutCallingDaemon()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        var library = new Mock<ILibraryManager>(MockBehavior.Strict);
        library.Setup(manager => manager.GetItemById<BaseItem>(itemId, userId))
            .Returns(new Audio { Path = "/music/track.mp3" });
        var controller = CreateController(daemon.Object, library.Object, userId);

        var result = await controller.GetItemFilterConfiguration(itemId, CancellationToken.None);

        Assert.NotNull(result.Value);
        Assert.False(result.Value!.FiltersEnabled);
        Assert.Null(result.Value.Warning);
        Assert.Empty(result.Value.Intervals);
        daemon.VerifyNoOtherCalls();
    }

    [Fact]
    public async Task EnqueueAnalysisRejectsAudioItems()
    {
        var itemId = Guid.NewGuid();
        var userId = Guid.NewGuid();
        var daemon = new Mock<IDaemonClient>(MockBehavior.Strict);
        var library = new Mock<ILibraryManager>(MockBehavior.Strict);
        library.Setup(manager => manager.GetItemById<BaseItem>(itemId))
            .Returns(new Audio { Path = "/music/track.mp3" });
        var controller = CreateController(daemon.Object, library.Object, userId);

        var result = await controller.EnqueueAnalysis(
            new EnqueueAnalysisRequest(itemId, Force: true),
            CancellationToken.None);

        Assert.IsType<BadRequestObjectResult>(result.Result);
        daemon.VerifyNoOtherCalls();
    }

    private static BrowserFilterConfigDto EmptyConfiguration()
        => new(
            new CategoryActionsDto(new Dictionary<string, string>()),
            new CategoryActionsDto(new Dictionary<string, string>()),
            Array.Empty<BrowserIntervalDto>(),
            "ready",
            Array.Empty<FilterProblemDto>(),
            true,
            null);

    private static Mock<ILibraryManager> VisibleItemLibrary(Guid itemId, Guid userId)
    {
        var library = new Mock<ILibraryManager>(MockBehavior.Strict);
        library.Setup(manager => manager.GetItemById<BaseItem>(itemId, userId))
            .Returns(new Movie { Path = "/media/movie.mp4" });
        return library;
    }

    private static ContentFilterController CreateController(
        IDaemonClient daemon,
        ILibraryManager library,
        Guid userId)
    {
        var principal = new ClaimsPrincipal(
            new ClaimsIdentity(
                new[] { new Claim("Jellyfin-UserId", userId.ToString("N")) },
                "test"));
        return new ContentFilterController(daemon, library)
        {
            ControllerContext = new ControllerContext
            {
                HttpContext = new DefaultHttpContext { User = principal },
            },
        };
    }
}
