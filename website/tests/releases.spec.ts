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

for (const width of [1280, 390]) {
  test(`0.61 changelog links reach the inline upgrade guide at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/releases/0.61.0/");
    const guide = page.getByRole("region", { name: "Upgrade guide", exact: true });
    await expect(guide).toBeVisible();
    await expect(page.locator('main a[href^="/highlights/2026-10-29-0-61-0-upgrade-guide/"]')).toHaveCount(0);

    await page.getByRole("link", { name: "Read the upgrade guide for this release." }).click();
    await expect(page).toHaveURL(/#upgrade-guide$/);
    await expect(guide.getByRole("heading", { name: "Upgrade guide", exact: true })).toBeInViewport();

    const vectorMigration = guide.locator("#vector-sink-default-https");
    await page.locator('.changelog-card > a[href="#vector-sink-default-https"]').click();
    await expect(page).toHaveURL(/\/releases\/0\.61\.0\/#vector-sink-default-https$/);
    await expect(vectorMigration).toBeInViewport();
    await expect(vectorMigration).toHaveJSProperty("tagName", "H3");
    await expect(guide).toContainText("Setting tls.enabled: false alone does not restore");

    const vrlEntry = page.locator(".vrl-changelog-entry").filter({
      has: page.locator('a[href="https://github.com/vectordotdev/vrl/pull/2001"]')
    });
    await expect(page.locator(".vrl-changelog-entry")).toHaveCount(6);
    await expect(vrlEntry.locator("pre")).toHaveCount(0);
    await expect(vrlEntry.getByRole("link", { name: "suryakant-source", exact: true })).toBeVisible();
    await vrlEntry.getByRole("link", { name: "migration instructions" }).click();
    await expect(page).toHaveURL(/\/releases\/0\.61\.0\/#vrl-pr-2001$/);
    await expect(guide.locator("#vrl-pr-2001")).toBeInViewport();
    await expect(guide.locator('pre:has(code[data-lang="vrl"])')).toHaveText([
      'to_unix_timestamp(ts, unit: "nanoseconds")',
      'to_unix_timestamp!(ts, unit: "nanoseconds")',
      'to_unix_timestamp(ts, unit: "nanoseconds") ?? 0'
    ]);
    await expect(page.getByText(/The conversion can fail at runtime when the timestamp is out of range/)).toHaveCount(
      1
    );

    const headings = await page
      .locator("main :is(h2, h3, h4, h5)")
      .evaluateAll((elements) => elements.map((element) => element.id));
    expect(headings.indexOf("upgrade-guide")).toBeGreaterThan(headings.indexOf("vrl-fixes"));
    expect(new Set(headings).size).toBe(headings.length);

    await page.getByRole("button", { name: "Toggle dark mode" }).click();
    await expect(guide).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  });
}

test("standalone upgrade-guide anchors remain available and older releases stay unchanged", async ({ page }) => {
  await page.goto("/highlights/2026-10-29-0-61-0-upgrade-guide/#vector-sink-default-https");
  await expect(page.locator("#vector-sink-default-https")).toBeInViewport();
  await expect(page.locator("#vector-sink-default-https")).toHaveJSProperty("tagName", "H2");
  await expect(page.locator("#vrl-to-unix-timestamp-nanoseconds")).toBeAttached();
  await expect(page.locator('pre:has(code[data-lang="vrl"])')).toHaveCount(3);

  await page.goto("/releases/0.61.0/#vrl-to-unix-timestamp-nanoseconds");
  await expect(page.locator("#vrl-pr-2001")).toBeInViewport();

  await page.goto("/releases/0.60.0/");
  await expect(page.locator(".upgrade-guide")).toHaveCount(0);
  const guideLink = page.locator('.changelog-card a[href^="/highlights/2026-10-19-0-60-0-upgrade-guide/#"]').first();
  await expect(guideLink).toBeVisible();
  await guideLink.click();
  await expect(page).toHaveURL(/\/highlights\/2026-10-19-0-60-0-upgrade-guide\/#/);
});
