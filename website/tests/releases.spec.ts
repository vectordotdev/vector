import { test, expect } from "./fixtures";

for (const width of [1280, 390]) {
  test(`multiline VRL entries stay inside their cards at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/releases/0.59.0/");
    const changelog = page.locator(".vrl-changelog");
    const entry = changelog.locator(".vrl-changelog-entry").filter({
      has: page.locator('a[href="https://github.com/vectordotdev/vrl/pull/1861"]')
    });

    await expect(changelog.locator(".vrl-changelog-entry")).toHaveCount(8);
    await expect(entry).toContainText("hard compile error instead of a runtime failure");
    await expect(entry.locator("pre")).toHaveText([
      'join!(["sources", "transforms", "sinks"], separator: ", ")',
      'join(["sources", "transforms", "sinks"], separator: ", ")'
    ]);
    await expect(entry.getByRole("link", { name: "pront", exact: true })).toBeVisible();

    const card = (await entry.boundingBox())!;
    expect(card.x).toBeGreaterThanOrEqual(0);
    expect(card.x + card.width).toBeLessThanOrEqual(width);
    for (const block of await entry.locator("pre, p").all()) {
      const box = (await block.boundingBox())!;
      expect(box.x).toBeGreaterThan(card.x);
      expect(box.x + box.width).toBeLessThan(card.x + card.width);
      expect(box.y + box.height).toBeLessThan(card.y + card.height);
    }
    await expect(entry).toHaveCSS("border-top-width", "1px");
    await expect(entry).toHaveCSS("border-bottom-width", "1px");
    const background = await entry.evaluate((element) => getComputedStyle(element).backgroundColor);
    await page.getByRole("button", { name: "Toggle dark mode" }).click();
    await expect(entry).not.toHaveCSS("background-color", background);
  });
}

test("older VRL migration notes and attribution formats retain their content", async ({ page }) => {
  await page.goto("/releases/0.59.0/");
  const migration = page.locator(".vrl-changelog-entry").filter({
    has: page.locator('a[href="https://github.com/vectordotdev/vrl/pull/1861"]')
  });
  await expect(migration.locator("pre")).toHaveCount(2);
  await expect(migration).toContainText("join");
  await expect(page.locator(".vrl-changelog-entry")).toHaveCount(8);

  await page.goto("/releases/0.56.0/");
  await expect(page.locator(".vrl-changelog-entry")).toHaveCount(14);
  await expect(page.locator('.vrl-changelog-entry del a[href$="/pull/1774"]')).toBeVisible();
  await expect(page.locator("#vrl-fixes")).toHaveCount(1);
  await expect(page.locator("#vrl-fixes-1")).toHaveCount(1);

  await page.goto("/releases/0.55.0/");
  const legacy = page.locator(".vrl-changelog");
  await expect(legacy).toContainText("encode_csv");
  await expect(legacy).toContainText("authors: armleth");
  await expect(legacy.locator('a[href="https://github.com/vectordotdev/vrl/pull/1649"]')).toBeVisible();
});
