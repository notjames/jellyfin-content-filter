import type { EffectiveInterval } from "./types";

const EPSILON = 0.08;

export class PlaybackScheduler {
  private video?: HTMLVideoElement;
  private intervals: EffectiveInterval[] = [];
  private timer?: number;
  private mutedBeforeFilter?: boolean;
  private lastSeekTarget = -1;
  private lastSeekAt = 0;
  private readonly evaluateBound = () => this.evaluate();

  attach(video: HTMLVideoElement): void {
    if (this.video === video) return;
    this.detach();
    this.video = video;
    for (const event of ["timeupdate", "seeking", "seeked", "playing", "ratechange", "loadedmetadata"]) {
      video.addEventListener(event, this.evaluateBound);
    }
    this.evaluate();
  }

  setIntervals(intervals: EffectiveInterval[]): void {
    this.intervals = intervals;
    this.lastSeekTarget = -1;
    this.evaluate();
  }

  detach(): void {
    if (!this.video) return;
    for (const event of ["timeupdate", "seeking", "seeked", "playing", "ratechange", "loadedmetadata"]) {
      this.video.removeEventListener(event, this.evaluateBound);
    }
    this.restoreMute();
    this.clearTimer();
    this.video = undefined;
  }

  evaluate(): void {
    const video = this.video;
    if (!video || !Number.isFinite(video.currentTime)) return;
    this.clearTimer();

    const now = video.currentTime;
    const active = this.intervals.filter((entry) => now >= entry.start - EPSILON && now < entry.end - EPSILON);
    const skips = active.filter((entry) => entry.action === "skip");
    if (skips.length) {
      const target = this.skipClosure(Math.max(...skips.map((entry) => entry.end)));
      const wallNow = Date.now();
      if (Math.abs(target - this.lastSeekTarget) > EPSILON || wallNow - this.lastSeekAt > 750) {
        this.lastSeekTarget = target;
        this.lastSeekAt = wallNow;
        video.currentTime = Math.min(target + EPSILON, Number.isFinite(video.duration) ? video.duration : target + EPSILON);
      }
      this.timer = window.setTimeout(this.evaluateBound, 300);
      return;
    }

    this.setFilterMute(active.some((entry) => entry.action === "mute"));
    const nextBoundary = this.nextBoundary(now);
    if (nextBoundary !== undefined && !video.paused) {
      const delay = Math.max(20, ((nextBoundary - now) / Math.max(video.playbackRate, 0.1)) * 1000 - 30);
      this.timer = window.setTimeout(this.evaluateBound, Math.min(delay, 1000));
    }
  }

  private skipClosure(initialEnd: number): number {
    let end = initialEnd;
    let changed = true;
    while (changed) {
      changed = false;
      for (const entry of this.intervals) {
        if (entry.action === "skip" && entry.start <= end + EPSILON && entry.end > end) {
          end = entry.end;
          changed = true;
        }
      }
    }
    return end;
  }

  private nextBoundary(now: number): number | undefined {
    return this.intervals
      .flatMap((entry) => [entry.start, entry.end])
      .filter((boundary) => boundary > now + EPSILON)
      .sort((a, b) => a - b)[0];
  }

  private setFilterMute(enabled: boolean): void {
    if (!this.video) return;
    if (enabled && this.mutedBeforeFilter === undefined) {
      this.mutedBeforeFilter = this.video.muted;
      this.video.muted = true;
    } else if (!enabled) {
      this.restoreMute();
    }
  }

  private restoreMute(): void {
    if (this.video && this.mutedBeforeFilter !== undefined) this.video.muted = this.mutedBeforeFilter;
    this.mutedBeforeFilter = undefined;
  }

  private clearTimer(): void {
    if (this.timer !== undefined) window.clearTimeout(this.timer);
    this.timer = undefined;
  }
}
