import { afterEach, describe, expect, it } from "vitest";
import { sessionFromGlobalApiClient, sessionFromPlaybackRequest } from "../src/api";

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

describe("sessionFromGlobalApiClient", () => {
  afterEach(() => {
    delete (window as typeof window & { ApiClient?: unknown }).ApiClient;
  });

  it("creates an authenticated fallback session for native playback", () => {
    (window as typeof window & { ApiClient?: unknown }).ApiClient = {
      getUrl: (path: string) => `https://media.example/jellyfin${path}`,
      accessToken: () => "secret"
    };

    const session = sessionFromGlobalApiClient();

    expect(session?.baseUrl).toBe("https://media.example/jellyfin");
    expect(session?.headers.get("X-Emby-Token")).toBe("secret");
  });
});
