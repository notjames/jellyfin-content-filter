import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopPlaybackScheduler } from "../src/scheduler";
import type { NativePlayer, NativeSignal } from "../src/native";

class Signal<T extends unknown[] = []> implements NativeSignal<T> {
  private readonly callbacks = new Set<(...args: T) => void>();
  connect(callback: (...args: T) => void): void { this.callbacks.add(callback); }
  disconnect(callback: (...args: T) => void): void { this.callbacks.delete(callback); }
  emit(...args: T): void { this.callbacks.forEach((callback) => callback(...args)); }
}

function player(): NativePlayer & { seeks: number[]; mutes: boolean[] } {
  return {
    seeks: [],
    mutes: [],
    positionUpdate: new Signal<[number]>(),
    finished: new Signal(),
    getPosition: () => 0,
    seekTo(positionMs) { this.seeks.push(positionMs); },
    setMuted(muted) { this.mutes.push(muted); }
  };
}

describe("DesktopPlaybackScheduler", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    window._mpvVideoPlayerInstance = { isMuted: () => false };
  });

  afterEach(() => {
    vi.useRealTimers();
    delete window._mpvVideoPlayerInstance;
  });

  it("seeks native mpv in milliseconds without touching HTML video", () => {
    const native = player();
    const scheduler = new DesktopPlaybackScheduler();
    scheduler.attach(native);
    scheduler.setIntervals([{ start: 10, end: 20, category: "violence", action: "skip" }]);
    scheduler.evaluatePosition(12_000);
    expect(native.seeks).toEqual([20_080]);
    expect(document.querySelector("video")).toBeNull();
    scheduler.detach();
  });

  it("restores the user's mute state after a filtered interval", async () => {
    const native = player();
    const scheduler = new DesktopPlaybackScheduler();
    scheduler.attach(native);
    scheduler.setIntervals([{ start: 1, end: 2, category: "language", action: "mute" }]);
    scheduler.evaluatePosition(1_500);
    await Promise.resolve();
    expect(native.mutes).toEqual([true]);
    scheduler.evaluatePosition(2_500);
    expect(native.mutes).toEqual([true, false]);
    scheduler.detach();
  });
});
