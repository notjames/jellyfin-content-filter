import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FilterApi } from "../src/api";
import { FilterUi } from "../src/ui";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  document.querySelector("#jcf-root")?.remove();
  document.querySelector(".videoOsdBottom")?.remove();
});

describe("FilterUi visibility", () => {
  it("is only visible while bound to an active title", () => {
    document.body.insertAdjacentHTML(
      "beforeend",
      '<div class="videoOsdBottom"><div class="buttons"><button class="btnVideoOsdSettings"></button></div></div>'
    );
    const ui = new FilterUi();
    const host = document.querySelector<HTMLElement>("#jcf-root");

    expect(host?.hidden).toBe(true);
    ui.bind("item", {} as FilterApi, {
      viewer: { categories: {} },
      intervals: []
    });
    expect(host?.hidden).toBe(false);
    const button = document.querySelector(".btnContentFilter");
    expect(button).not.toBeNull();
    expect(button?.nextElementSibling?.classList.contains("btnVideoOsdSettings")).toBe(true);

    ui.unbind();
    expect(host?.hidden).toBe(true);
    expect(document.querySelector(".btnContentFilter")).toBeNull();
  });
});

describe("FilterUi warnings", () => {
  it("auto-dismisses warnings after a delay", () => {
    const ui = new FilterUi();
    ui.warning("Playback is continuing unfiltered.");

    const warning = document.querySelector<HTMLElement>("#jcf-root")?.shadowRoot?.querySelector<HTMLElement>("#warning");
    expect(warning?.classList.contains("visible")).toBe(true);

    vi.advanceTimersByTime(12_600);
    expect(warning?.classList.contains("visible")).toBe(false);
    expect(warning?.style.display).toBe("none");
  });

  it("dismisses warnings immediately when the close button is clicked", () => {
    const ui = new FilterUi();
    ui.warning("Playback is continuing unfiltered.");

    const root = document.querySelector("#jcf-root")?.shadowRoot;
    root?.querySelector<HTMLButtonElement>("#warning-dismiss")?.click();
    vi.advanceTimersByTime(600);

    const warning = root?.querySelector<HTMLElement>("#warning");
    expect(warning?.classList.contains("visible")).toBe(false);
    expect(warning?.style.display).toBe("none");
  });
});
