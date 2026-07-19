import { FilterApi } from "../../web/src/api";
import { resolveIntervals } from "../../web/src/types";
import { FilterUi } from "../../web/src/ui";
import { DesktopPlaybackDetector, type DesktopPlaybackTarget } from "./detector";
import { isDesktopClient, resolveNativePlayer } from "./native";
import { DesktopPlaybackScheduler } from "./scheduler";

class DesktopContentFilterClient {
  private readonly detector = new DesktopPlaybackDetector();
  private readonly ui: FilterUi;
  private readonly scheduler: DesktopPlaybackScheduler;
  private generation = 0;

  constructor() {
    this.scheduler = new DesktopPlaybackScheduler((message) => this.ui.warning(message));
    this.ui = new FilterUi((config) => this.scheduler.setIntervals(resolveIntervals(config)));
  }

  async start(): Promise<void> {
    const player = await resolveNativePlayer();
    this.scheduler.attach(player);
    this.detector.addEventListener("playback", (event) => {
      void this.load((event as CustomEvent<DesktopPlaybackTarget>).detail);
    });
    this.detector.start();
  }

  private async load(target: DesktopPlaybackTarget): Promise<void> {
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

if (!window.JellyfinContentFilter?.started) {
  if (!isDesktopClient()) {
    console.warn("Jellyfin Content Filter desktop bundle ignored: no Jellyfin Desktop bridge found.");
  } else {
    window.JellyfinContentFilter = { started: true, target: "desktop" };
    const start = () => {
      void new DesktopContentFilterClient().start().catch((error) => {
        console.error("Jellyfin Content Filter desktop startup failed", error);
      });
    };
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
    else start();
  }
}
