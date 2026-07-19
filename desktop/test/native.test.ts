import { afterEach, describe, expect, it } from "vitest";
import { isDesktopClient, readNativeMute, resolveNativePlayer, type NativePlayer } from "../src/native";

const nativePlayer: NativePlayer = {
  getPosition: () => 0,
  seekTo: () => undefined,
  setMuted: () => undefined
};

afterEach(() => {
  delete window.api;
  delete window.apiPromise;
  delete window.jmpInfo;
  delete window.jmpNative;
  delete window._mpvVideoPlayerInstance;
});

describe("Jellyfin Desktop bridge", () => {
  it("detects and resolves the current CEF desktop API", async () => {
    window.jmpInfo = { mode: "desktop" };
    window.jmpNative = { playerSeek: () => undefined };
    window.api = { player: nativePlayer };
    expect(isDesktopClient()).toBe(true);
    await expect(resolveNativePlayer()).resolves.toBe(nativePlayer);
  });

  it("also resolves the legacy asynchronous API bridge", async () => {
    window.jmpInfo = {};
    window.apiPromise = Promise.resolve({ player: nativePlayer });
    expect(isDesktopClient()).toBe(true);
    await expect(resolveNativePlayer()).resolves.toBe(nativePlayer);
  });

  it("reads mute state from the desktop video-player mirror", async () => {
    window._mpvVideoPlayerInstance = { isMuted: () => true };
    await expect(readNativeMute(nativePlayer)).resolves.toBe(true);
  });
});
