import { test, expect } from "./fixtures";

for (const version of ["0.58.0", "0.59.0", "0.60.0", "0.61.0"]) {
  test(`generated description fills the listing and release page for ${version}`, async ({ page }) => {
    await page.goto("/releases/");
    const card = page.locator(`a[href="/releases/${version}/"]`).filter({ hasText: `Version ${version}` });
    const listing = card.locator("xpath=following-sibling::div[1]").locator(".release-description");
    await expect(listing).toContainText(`is happy to announce Vector ${version}!`);
    await expect(listing).toContainText(/Thanks to the \d+ contributors who made the \d+ changes/);
    await expect(listing.getByRole("link", { name: "upgrade guide", exact: true })).toBeVisible();
    await listing.getByRole("link", { name: "View the full release notes" }).click();
    await expect(page).toHaveURL(`/releases/${version}/`);

    const description = page.locator(".release-description");
    await expect(description).toContainText(`is happy to announce Vector ${version}!`);
    await expect(description.getByRole("link", { name: "View the full release notes" })).toHaveCount(0);
    for (const link of await description.locator("li a").all()) {
      const label = await link.innerText();
      const href = (await link.getAttribute("href"))!;
      const anchor = new URL(href, page.url()).hash;
      await expect(page.locator(anchor)).toHaveText(label);
    }
    await description.getByRole("link", { name: "upgrade guide", exact: true }).click();
    await expect(
      page
        .locator("h1, h2")
        .filter({ hasText: /Upgrade Guide/i })
        .first()
    ).toBeVisible();
  });
}

test("handwritten release descriptions retain their content", async ({ page }) => {
  for (const path of ["/releases/", "/releases/0.57.0/"]) {
    await page.goto(path);
    const description = page.locator(".release-description").filter({ hasText: "over-restrict now and relax later" });
    await expect(description).toContainText("Breaking Changes");
    await expect(description).not.toContainText("is happy to announce Vector");
    await expect(description).toContainText("environment variable interpolation is now disabled by default");
  }
});

for (const width of [1280, 390]) {
  test(`VRL migration links to its upgrade-guide card at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/releases/0.61.0/#vrl-fixes");
    const changelog = page.locator(".vrl-changelog");
    const entry = changelog.locator(".vrl-changelog-entry").filter({
      has: page.locator('a[href="https://github.com/vectordotdev/vrl/pull/2001"]')
    });

    await expect(changelog.locator(".vrl-changelog-entry")).toHaveCount(6);
    await expect(entry).toContainText("the compiler previously rejected error handling for this call as unnecessary");
    await expect(entry.locator("pre")).toHaveCount(0);
    await entry.getByRole("link", { name: "upgrade guide", exact: true }).click();
    await expect(page).toHaveURL(/#vrl-to-unix-timestamp-nanoseconds$/);
    const migration = page.locator(".upgrade-guide-entry").filter({
      has: page.locator("#vrl-to-unix-timestamp-nanoseconds")
    });
    await expect(migration).toContainText("they no longer compile until the error is handled");
    await expect(migration.locator("pre")).toHaveText([
      'to_unix_timestamp(ts, unit: "nanoseconds")',
      'to_unix_timestamp!(ts, unit: "nanoseconds")',
      'to_unix_timestamp(ts, unit: "nanoseconds") ?? 0'
    ]);
    await expect(entry.getByRole("link", { name: "suryakant-source", exact: true })).toBeVisible();

    await expect(migration.getByRole("link", { name: "suryakant-source", exact: true })).toBeVisible();
    const card = (await migration.boundingBox())!;
    expect(card.x).toBeGreaterThanOrEqual(0);
    expect(card.x + card.width).toBeLessThanOrEqual(width);
    for (const block of await migration.locator("pre, p").all()) {
      const box = (await block.boundingBox())!;
      expect(box.x).toBeGreaterThan(card.x);
      expect(box.x + box.width).toBeLessThan(card.x + card.width);
      expect(box.y + box.height).toBeLessThan(card.y + card.height);
    }
    await expect(migration).toHaveCSS("border-top-width", "1px");
    await expect(migration).toHaveCSS("border-bottom-width", "1px");
    const background = await migration.evaluate((element) => getComputedStyle(element).backgroundColor);
    await page.getByRole("button", { name: "Toggle dark mode" }).click();
    await expect(migration).not.toHaveCSS("background-color", background);
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

test("inline guide subheadings have scoped links", async ({ page }) => {
  await page.goto("/releases/0.61.0/");
  const guide = page.locator(".upgrade-guide");
  await expect(guide.locator("#vector-sink-default-https")).toBeVisible();
  const headings = await guide.locator("h4, h5, h6").evaluateAll((elements) => elements.map((element) => element.id));
  expect(headings.length).toBeGreaterThan(0);
  expect(headings.every((id) => id.startsWith("upgrade-guide-"))).toBe(true);

  const old = guide.locator("#upgrade-guide-old");
  await expect(old.locator('a[href="#upgrade-guide-old"]')).toHaveCount(1);
  const tocLink = page.locator('#toc a[href="#upgrade-guide-old"]');
  await tocLink.click();
  await expect(page).toHaveURL(/#upgrade-guide-old$/);
  await expect(old).toBeInViewport();
});
