import { test, expect } from "./fixtures";

test("deprecations TOC links to notices and follows sorting", async ({ page }) => {
  await page.goto("/deprecations/");
  const toc = page.locator("#toc");
  await expect(toc.locator(":scope > ol > li > a")).toHaveText(["Active Deprecations", "Past Deprecations"]);
  await expect(toc.getByRole("link", { name: "Settings with an enabled field", exact: true })).toHaveCount(0);

  const notices = toc.locator(":scope > ol > li").first().locator("ol a");
  await expect(notices.first()).toBeVisible();
  const titles = await notices.allTextContents();
  await page.getByRole("button", { name: /Sorted by deprecation version/ }).click();
  await expect(notices).toHaveText(titles.reverse());

  const target = await notices.first().getAttribute("href");
  await notices.first().click();
  await expect(page.locator(target!)).toBeInViewport();
});
