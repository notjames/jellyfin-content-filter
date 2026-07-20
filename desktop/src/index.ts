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
    this.scheduler = new DesktopPlaybackScheduler(
      (message) => this.ui.warning(message),
      () => this.deactivate()
    );
    this.ui = new FilterUi((config) => this.scheduler.setIntervals(resolveIntervals(config)));
  }

  async start(): Promise<void> {
    this.detector.addEventListener("playback", (event) => {
      void this.load((event as CustomEvent<DesktopPlaybackTarget>).detail);
    });
    this.detector.addEventListener("stopped", () => this.deactivate());
    this.detector.start();
    const player = await resolveNativePlayer();
    this.scheduler.attach(player);
  }

  private async load(target: DesktopPlaybackTarget): Promise<void> {
    const generation = ++this.generation;
    const api = new FilterApi(target.session);
    this.scheduler.setIntervals([]);
    this.ui.unbind();
    try {
      const config = await api.getConfig(target.itemId);
      if (generation !== this.generation) return;
      this.scheduler.setIntervals(resolveIntervals(config));
      this.ui.bind(target.itemId, api, config);
      if (config.warning) this.ui.warning(config.warning);
    } catch (error) {
      if (generation !== this.generation) return;
      this.scheduler.setIntervals([]);
      this.ui.unbind();
      console.warn(`Content filter could not load (${String(error)}). Playback is continuing unfiltered.`);
    }
  }

  private deactivate(): void {
    this.generation++;
    this.scheduler.setIntervals([]);
    this.ui.unbind();
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
