import { afterEach, describe, expect, it } from "vitest";
import type { FilterApi } from "../src/api";
import { FilterUi } from "../src/ui";

afterEach(() => {
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
