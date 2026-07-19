import { describe, expect, it } from "vitest";
import { sessionFromPlaybackRequest } from "../src/api";

describe("sessionFromPlaybackRequest", () => {
  it("preserves Jellyfin's reverse-proxy base path and auth", () => {
    const session = sessionFromPlaybackRequest(
      "https://media.example/jellyfin/Items/abc%201/PlaybackInfo",
      { headers: { Authorization: "MediaBrowser Token=secret" } }
    );
    expect(session?.baseUrl).toBe("https://media.example/jellyfin");
    expect(session?.headers.get("Authorization")).toBe("MediaBrowser Token=secret");
  });

  it("ignores unrelated API requests", () => {
    expect(sessionFromPlaybackRequest("/jellyfin/System/Info")).toBeNull();
  });
});
