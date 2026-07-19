import { FilterApi } from "./api";
import { PlaybackDetector, type PlaybackTarget } from "./detector";
import { PlaybackScheduler } from "./scheduler";
import { resolveIntervals } from "./types";
import { FilterUi } from "./ui";

class ContentFilterClient {
  private readonly detector = new PlaybackDetector();
  private readonly scheduler = new PlaybackScheduler();
  private readonly ui = new FilterUi((config) => this.scheduler.setIntervals(resolveIntervals(config)));
  private generation = 0;

  start(): void {
    this.detector.addEventListener("video", (event) => {
      this.scheduler.attach((event as CustomEvent<HTMLVideoElement>).detail);
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
    try {
      const config = await api.getConfig(target.itemId);
      if (generation !== this.generation) return;
      this.scheduler.setIntervals(resolveIntervals(config));
      this.ui.bind(target.itemId, api, config);
    } catch (error) {
      if (generation !== this.generation) return;
      this.scheduler.setIntervals([]);
      this.ui.warning(`Content filter could not load (${String(error)}). Playback is continuing unfiltered.`);
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
