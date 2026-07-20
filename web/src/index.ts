import { FilterApi } from "./api";
import { PlaybackDetector, type PlaybackTarget } from "./detector";
import { PlaybackScheduler } from "./scheduler";
import { SubtitleMasker } from "./subtitles";
import { resolveIntervals } from "./types";
import { FilterUi } from "./ui";

class ContentFilterClient {
  private readonly detector = new PlaybackDetector();
  private readonly scheduler = new PlaybackScheduler();
  private readonly subtitles = new SubtitleMasker();
  private readonly ui = new FilterUi((config) => {
    const intervals = resolveIntervals(config);
    this.scheduler.setIntervals(intervals);
    this.subtitles.setIntervals(intervals);
  });
  private generation = 0;

  start(): void {
    this.detector.addEventListener("video", (event) => {
      const video = (event as CustomEvent<HTMLVideoElement | undefined>).detail;
      if (video) {
        this.scheduler.attach(video);
        this.subtitles.attach(video);
      } else {
        this.generation++;
        this.scheduler.setIntervals([]);
        this.subtitles.setIntervals([]);
        this.scheduler.detach();
        this.subtitles.detach();
        this.ui.unbind();
      }
    });
    this.detector.addEventListener("playback", (event) => {
      void this.load((event as CustomEvent<PlaybackTarget>).detail);
    });
    this.detector.start();
  }

  private async load(target: PlaybackTarget): Promise<void> {
    const generation = ++this.generation;
    const api = new FilterApi(target.session);
    this.scheduler.setIntervals([]);
    this.subtitles.setIntervals([]);
    this.ui.unbind();
    try {
      const config = await api.getConfig(target.itemId);
      if (generation !== this.generation) return;
      const intervals = resolveIntervals(config);
      this.scheduler.setIntervals(intervals);
      this.subtitles.setIntervals(intervals);
      this.ui.bind(target.itemId, api, config);
      if (config.warning) this.ui.warning(config.warning);
    } catch (error) {
      if (generation !== this.generation) return;
      this.scheduler.setIntervals([]);
      this.subtitles.setIntervals([]);
      this.ui.unbind();
      console.warn(`Content filter could not load (${String(error)}). Playback is continuing unfiltered.`);
    }
  }
}

declare global {
  interface Window {
    JellyfinContentFilter?: { started: boolean; target?: "web" | "desktop" };
  }
}

const desktopGlobals = window as typeof window & {
  jmpInfo?: { mode?: string };
  apiPromise?: Promise<unknown>;
};
const isDesktopHost = Boolean(
  desktopGlobals.jmpInfo && (desktopGlobals.jmpInfo.mode === "desktop" || desktopGlobals.apiPromise)
);

if (!window.JellyfinContentFilter?.started && !isDesktopHost) {
  window.JellyfinContentFilter = { started: true, target: "web" };
  const start = () => new ContentFilterClient().start();
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
  else start();
}
