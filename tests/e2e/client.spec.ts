import { expect, test } from "@playwright/test";

test("loads title policy and skips an active interval", async ({ page }) => {
  await page.goto("/tests/e2e/fixture.html");
  await expect(page.locator("#jcf-root")).toHaveCount(1);
  await page.waitForTimeout(100);
  const currentTime = await page.locator("video").evaluate((video: HTMLVideoElement) => {
    video.currentTime = 12;
    video.dispatchEvent(new Event("seeked"));
    return video.currentTime;
  });
  expect(currentTime).toBeGreaterThan(20);
});

test("opens the settings surface", async ({ page }) => {
  await page.goto("/tests/e2e/fixture.html");
  const host = page.locator("#jcf-root");
  await host.evaluate((element) => {
    (element.shadowRoot?.querySelector("#open") as HTMLButtonElement).click();
  });
  await expect(host).toHaveJSProperty("id", "jcf-root");
  expect(await host.evaluate((element) => element.shadowRoot?.querySelector("dialog")?.open)).toBe(true);
});
