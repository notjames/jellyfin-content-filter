import { describe, expect, it, vi } from "vitest";
import { PlaybackScheduler } from "../src/scheduler";

function videoAt(time: number, muted = false): HTMLVideoElement {
  const video = document.createElement("video");
  Object.defineProperties(video, {
    currentTime: { value: time, writable: true },
    duration: { value: 100, writable: true },
    paused: { value: true, writable: true },
    playbackRate: { value: 1, writable: true },
    muted: { value: muted, writable: true }
  });
  return video;
}

describe("PlaybackScheduler", () => {
  it("immediately skips when seeking into an interval and closes overlaps", () => {
    vi.useFakeTimers();
    const video = videoAt(12);
    const scheduler = new PlaybackScheduler();
    scheduler.attach(video);
    scheduler.setIntervals([
      { start: 10, end: 15, category: "a", action: "skip" },
      { start: 14.9, end: 20, category: "b", action: "skip" }
    ]);
    expect(video.currentTime).toBeCloseTo(20.08);
    vi.useRealTimers();
  });

  it("restores the viewer's prior mute state", () => {
    const video = videoAt(5, false);
    const scheduler = new PlaybackScheduler();
    scheduler.attach(video);
    scheduler.setIntervals([{ start: 4, end: 6, category: "a", action: "mute" }]);
    expect(video.muted).toBe(true);
    video.currentTime = 7;
    scheduler.evaluate();
    expect(video.muted).toBe(false);
  });

  it("restores mute when the player element is replaced", () => {
    const first = videoAt(5, false);
    const second = videoAt(0, true);
    const scheduler = new PlaybackScheduler();
    scheduler.attach(first);
    scheduler.setIntervals([{ start: 4, end: 6, category: "a", action: "mute" }]);
    scheduler.attach(second);
    expect(first.muted).toBe(false);
    expect(second.muted).toBe(true);
  });
});
