import { expect, test } from "@playwright/test";

test("desktop target controls native mpv without an HTML video element", async ({ page }) => {
  await page.goto("/tests/e2e/desktop-fixture.html");
  await expect(page.locator("#jcf-root")).toHaveCount(1);
  await expect(page.locator("video")).toHaveCount(0);
  await page.waitForTimeout(100);
  await page.evaluate(() => {
    const desktopWindow = window as typeof window & {
      __desktopState: { position: number };
      __desktopPlayer: { positionUpdate: { emit: (position: number) => void } };
    };
    desktopWindow.__desktopState.position = 12_000;
    desktopWindow.__desktopPlayer.positionUpdate.emit(12_000);
  });
  const seeks = await page.evaluate(
    () => (window as typeof window & { __desktopState: { seeks: number[] } }).__desktopState.seeks
  );
  expect(seeks).toEqual([20_080]);
  expect(await page.evaluate(
    () => (window as typeof window & { JellyfinContentFilter?: { target?: string } }).JellyfinContentFilter?.target
  )).toBe("desktop");
});

test("web target still uses the HTML5 player", async ({ page }) => {
  await page.goto("/tests/e2e/fixture.html");
  expect(await page.evaluate(
    () => (window as typeof window & { JellyfinContentFilter?: { target?: string } }).JellyfinContentFilter?.target
  )).toBe("web");
  await expect(page.locator("video")).toHaveCount(1);
});
