import type { EffectiveInterval } from "../../web/src/types";
import { readNativeMute, type NativePlayer, type NativeSignal } from "./native";

const EPSILON_SECONDS = 0.08;
const POLL_MS = 100;

export class DesktopPlaybackScheduler {
  private player?: NativePlayer;
  private intervals: EffectiveInterval[] = [];
  private timer?: number;
  private lastSeekTarget = -1;
  private lastSeekAt = 0;
  private originalMute?: boolean;
  private ownsMute = false;
  private wantsMute = false;
  private muteGeneration = 0;
  private warnedAboutMute = false;
  private readonly onPosition = (positionMs: number) => this.evaluatePosition(positionMs);
  private readonly onStopped = () => this.resetPlaybackState();

  constructor(private readonly onWarning?: (message: string) => void) {}

  attach(player: NativePlayer): void {
    if (this.player === player) return;
    this.detach();
    this.player = player;
    this.connect(player.positionUpdate, this.onPosition);
    this.connect(player.finished, this.onStopped);
    this.connect(player.canceled, this.onStopped);
    this.connect(player.stopped, this.onStopped);
    this.poll();
  }

  setIntervals(intervals: EffectiveInterval[]): void {
    this.intervals = intervals;
    this.lastSeekTarget = -1;
    this.poll();
  }

  detach(): void {
    const player = this.player;
    if (!player) return;
    this.disconnect(player.positionUpdate, this.onPosition);
    this.disconnect(player.finished, this.onStopped);
    this.disconnect(player.canceled, this.onStopped);
    this.disconnect(player.stopped, this.onStopped);
    this.restoreMute();
    this.clearTimer();
    this.player = undefined;
  }

  evaluatePosition(positionMs: number): void {
    if (!this.player || !Number.isFinite(positionMs) || positionMs < 0) return;
    const now = positionMs / 1000;
    const active = this.intervals.filter(
      (entry) => now >= entry.start - EPSILON_SECONDS && now < entry.end - EPSILON_SECONDS
    );
    const skips = active.filter((entry) => entry.action === "skip");
    if (skips.length) {
      void this.setFilterMute(false);
      const target = this.skipClosure(Math.max(...skips.map((entry) => entry.end)));
      const wallNow = Date.now();
      if (Math.abs(target - this.lastSeekTarget) > EPSILON_SECONDS || wallNow - this.lastSeekAt > 750) {
        this.lastSeekTarget = target;
        this.lastSeekAt = wallNow;
        this.player.seekTo(Math.round((target + EPSILON_SECONDS) * 1000));
      }
      return;
    }
    void this.setFilterMute(active.some((entry) => entry.action === "mute"));
  }

  private poll(): void {
    this.clearTimer();
    const player = this.player;
    if (!player) return;
    let handled = false;
    const accept = (positionMs: number) => {
      if (handled) return;
      handled = true;
      this.evaluatePosition(positionMs);
    };
    const returned = player.getPosition(accept);
    if (typeof returned === "number") accept(returned);
    this.timer = window.setTimeout(() => this.poll(), POLL_MS);
  }

  private skipClosure(initialEnd: number): number {
    let end = initialEnd;
    let changed = true;
    while (changed) {
      changed = false;
      for (const entry of this.intervals) {
        if (entry.action === "skip" && entry.start <= end + EPSILON_SECONDS && entry.end > end) {
          end = entry.end;
          changed = true;
        }
      }
    }
    return end;
  }

  private async setFilterMute(enabled: boolean): Promise<void> {
    this.wantsMute = enabled;
    const generation = ++this.muteGeneration;
    if (!enabled) {
      this.restoreMute();
      return;
    }
    if (this.ownsMute || !this.player) return;

    const original = await readNativeMute(this.player);
    if (generation !== this.muteGeneration || !this.wantsMute || !this.player) return;
    if (original === undefined) {
      if (!this.warnedAboutMute) {
        this.warnedAboutMute = true;
        this.onWarning?.("Desktop mute state is unavailable; audio filters are disabled for this client.");
      }
      return;
    }
    this.originalMute = original;
    this.player.setMuted(true);
    this.ownsMute = true;
  }

  private restoreMute(): void {
    this.wantsMute = false;
    this.muteGeneration++;
    if (this.player && this.ownsMute && this.originalMute !== undefined) {
      this.player.setMuted(this.originalMute);
    }
    this.originalMute = undefined;
    this.ownsMute = false;
  }

  private resetPlaybackState(): void {
    this.lastSeekTarget = -1;
    this.restoreMute();
  }

  private connect<T extends unknown[]>(signal: NativeSignal<T> | undefined, callback: (...args: T) => void): void {
    signal?.connect(callback);
  }

  private disconnect<T extends unknown[]>(signal: NativeSignal<T> | undefined, callback: (...args: T) => void): void {
    signal?.disconnect(callback);
  }

  private clearTimer(): void {
    if (this.timer !== undefined) window.clearTimeout(this.timer);
    this.timer = undefined;
  }
}
