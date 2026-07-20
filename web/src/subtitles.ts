import { maskSubtitleText, type EffectiveInterval } from "./types";

const EPSILON = 0.08;

export class SubtitleMasker {
  private video?: HTMLVideoElement;
  private intervals: EffectiveInterval[] = [];
  private originals = new WeakMap<TextTrackCue, string>();
  private timer?: number;
  private readonly tick = () => this.apply();

  attach(video: HTMLVideoElement): void {
    if (this.video === video) return;
    this.detach();
    this.video = video;
    video.addEventListener("timeupdate", this.tick);
    video.addEventListener("seeked", this.tick);
    this.apply();
  }

  setIntervals(intervals: EffectiveInterval[]): void {
    this.intervals = intervals.filter((entry) => entry.action === "mute" || entry.action === "skip");
    this.apply();
  }

  detach(): void {
    if (!this.video) return;
    this.restoreAll();
    this.video.removeEventListener("timeupdate", this.tick);
    this.video.removeEventListener("seeked", this.tick);
    this.clearTimer();
    this.video = undefined;
  }

  private apply(): void {
    const video = this.video;
    if (!video || !Number.isFinite(video.currentTime)) return;
    this.clearTimer();
    const now = video.currentTime;
    const active = this.intervals.some(
      (entry) => now >= entry.start - EPSILON && now < entry.end - EPSILON
    );
    for (const track of Array.from(video.textTracks)) {
      if (track.mode === "disabled") continue;
      const cues = track.cues ? Array.from(track.cues) : [];
      for (const cue of cues) {
        if (!(cue instanceof VTTCue) && !("text" in cue)) continue;
        const textCue = cue as VTTCue;
        if (!this.originals.has(textCue)) this.originals.set(textCue, textCue.text);
        const original = this.originals.get(textCue) ?? textCue.text;
        const overlaps =
          active &&
          textCue.endTime > now - EPSILON &&
          textCue.startTime < now + EPSILON &&
          this.intervals.some(
            (entry) =>
              textCue.endTime > entry.start - EPSILON && textCue.startTime < entry.end + EPSILON
          );
        textCue.text = overlaps ? maskSubtitleText(original) : original;
      }
    }
    this.timer = window.setTimeout(this.tick, 200);
  }

  private restoreAll(): void {
    const video = this.video;
    if (!video) return;
    for (const track of Array.from(video.textTracks)) {
      const cues = track.cues ? Array.from(track.cues) : [];
      for (const cue of cues) {
        const textCue = cue as VTTCue;
        const original = this.originals.get(textCue);
        if (original !== undefined) textCue.text = original;
      }
    }
  }

  private clearTimer(): void {
    if (this.timer !== undefined) window.clearTimeout(this.timer);
    this.timer = undefined;
  }
}
