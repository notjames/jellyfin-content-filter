import { describe, expect, it } from "vitest";
import { maskSubtitleText, resolveIntervals } from "../src/types";

describe("resolveIntervals", () => {
  it("applies interval, title, viewer, and allow precedence", () => {
    const result = resolveIntervals({
      viewer: { categories: { violence: "mute", language: "skip" } },
      title: { categories: { violence: "skip" } },
      intervals: [
        { start: 3, end: 4, category: "unknown" },
        { start: 1, end: 2, category: "violence" },
        { start: 2, end: 3, category: "language", action: "mute" }
      ]
    });
    expect(result.map(({ category, action }) => [category, action])).toEqual([
      ["violence", "skip"],
      ["language", "mute"],
      ["unknown", "allow"]
    ]);
  });

  it("drops empty and inverted intervals", () => {
    expect(resolveIntervals({
      viewer: { categories: {} },
      intervals: [{ start: 2, end: 2, category: "x" }, { start: 3, end: 1, category: "x" }]
    })).toEqual([]);
  });

  it("returns no intervals when filters are disabled", () => {
    expect(resolveIntervals({
      viewer: { categories: { language: "mute" } },
      intervals: [{ start: 1, end: 2, category: "language" }],
      filters_enabled: false
    })).toEqual([]);
  });
});

describe("maskSubtitleText", () => {
  it("masks visible characters while preserving whitespace", () => {
    expect(maskSubtitleText("Oh damn it")).toBe("** **** **");
  });
});
