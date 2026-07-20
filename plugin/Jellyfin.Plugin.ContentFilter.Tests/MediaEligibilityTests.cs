using MediaBrowser.Controller.Entities;
using MediaBrowser.Controller.Entities.Audio;
using MediaBrowser.Controller.Entities.Movies;
using MediaBrowser.Controller.Entities.TV;
using Xunit;

namespace Jellyfin.Plugin.ContentFilter.Tests;

public sealed class MediaEligibilityTests
{
    [Fact]
    public void AcceptsMoviesAndEpisodes()
    {
        Assert.True(MediaEligibility.IsVideoFilterTarget(new Movie { Path = "/media/movie.mp4" }));
        Assert.True(MediaEligibility.IsVideoFilterTarget(new Episode { Path = "/media/show.mkv" }));
    }

    [Fact]
    public void RejectsAudioAndMusic()
    {
        Assert.False(MediaEligibility.IsVideoFilterTarget(new Audio { Path = "/music/song.mp3" }));
        Assert.False(MediaEligibility.IsVideoFilterTarget(new MusicAlbum { Path = "/music/album" }));
    }

    [Fact]
    public void RejectsMissingPathAndFolders()
    {
        Assert.False(MediaEligibility.IsVideoFilterTarget(null));
        Assert.False(MediaEligibility.IsVideoFilterTarget(new Movie()));
        Assert.False(MediaEligibility.IsVideoFilterTarget(new Folder { Path = "/media" }));
    }
}
