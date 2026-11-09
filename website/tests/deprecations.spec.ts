import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures";

const sections = [
  { id: "active-deprecations", title: "Active Deprecations", sort: "deprecation version" },
  { id: "past-deprecations", title: "Past Deprecations", sort: "removal version" }
];

const tocSection = (page: Page, title: string) =>
  page.locator("#toc > ol > li").filter({ has: page.getByRole("link", { name: title, exact: true }) });

test("deprecations TOC groups notices and links to their cards", async ({ page }) => {
  await page.goto("/deprecations/");
  const toc = page.locator("#toc");
  await expect(toc.locator(":scope > ol > li > a")).toHaveText(sections.map((section) => section.title));
  await expect(toc.getByRole("link", { name: "Settings with an enabled field", exact: true })).toHaveCount(0);
  await expect(toc).toHaveCSS("overflow-y", "auto");
  const bounds = await toc.boundingBox();
  expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(page.viewportSize()!.height);
  await expect(tocSection(page, "Active Deprecations").locator(":scope > ol")).toBeVisible();
  await expect(tocSection(page, "Past Deprecations").locator(":scope > ol")).toBeHidden();

  for (const section of sections) {
    const cards = page.locator(`#${section.id}`).locator("xpath=../..").locator(".changelog-card");
    const notices = await cards.evaluateAll((cards) =>
      cards.map((card) => ({
        href: `#${card.id}`,
        title: card.firstElementChild!.textContent!.trim()
      }))
    );
    expect(notices.length).toBeGreaterThan(0);
    await tocSection(page, section.title).locator(":scope > a").click();
    const links = tocSection(page, section.title).locator(":scope > ol > li > a");
    await expect(links.first()).toBeVisible();
    const otherSection = sections.find((other) => other.id !== section.id)!;
    await expect(tocSection(page, otherSection.title).locator(":scope > ol")).toBeHidden();
    await expect(links).toHaveText(notices.map((notice) => notice.title));
    await expect(links.first()).toHaveAttribute("title", notices[0].title);
    expect(
      await links.evaluateAll((links) =>
        links.every(
          (link) => link.getBoundingClientRect().height <= 2 * parseFloat(getComputedStyle(link).lineHeight) + 1
        )
      )
    ).toBe(true);
    expect(await links.evaluateAll((links) => links.map((link) => link.getAttribute("href")))).toEqual(
      notices.map((notice) => notice.href)
    );

    await links.last().click();
    await expect(page.locator(notices.at(-1)!.href)).toBeInViewport();
  }
});

test("deprecations TOC follows each section's sort order", async ({ page }) => {
  await page.goto("/deprecations/");
  for (const section of sections) {
    await tocSection(page, section.title).locator(":scope > a").click();
    const links = tocSection(page, section.title).locator(":scope > ol > li > a");
    await expect(links.first()).toBeVisible();
    const original = await links.allTextContents();
    expect(original.length).toBeGreaterThan(1);
    const sort = page.getByRole("button", { name: `Sorted by ${section.sort}, newest first. Reverse order.` });
    await sort.click();
    await expect(links).toHaveText([...original].reverse());
    await page.getByRole("button", { name: `Sorted by ${section.sort}, oldest first. Reverse order.` }).click();
    await expect(links).toHaveText(original);
  }
});

test("deprecations layout leaves room for navigation and content", async ({ page }) => {
  for (const width of [1024, 1280, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/deprecations/");
    const sidebar = await page.locator(".deprecations-toc").boundingBox();
    const content = await page.getByRole("main", { name: "Deprecations" }).boundingBox();
    expect(sidebar!.width).toBeGreaterThanOrEqual(256);
    expect(content!.width).toBeGreaterThanOrEqual(600);
    expect(sidebar!.x - (content!.x + content!.width)).toBeGreaterThanOrEqual(32);
    expect(
      await page.locator(".deprecations-layout").evaluate((layout) => layout.scrollWidth <= layout.clientWidth)
    ).toBe(true);
  }

  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/deprecations/");
  await expect(page.locator(".deprecations-toc")).toBeHidden();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("ordinary documentation retains its heading TOC", async ({ page }) => {
  await page.goto("/docs/setup/installation/package-managers/apt/");
  const headings = await page
    .locator("#page-content h2,#page-content h3,#page-content h4,#page-content h5")
    .evaluateAll((headings) => headings.map((heading) => `#${heading.id}`));
  expect(headings.length).toBeGreaterThan(2);
  const links = page.locator("#toc a");
  await expect(links).toHaveCount(headings.length);
  expect(await links.evaluateAll((links) => links.map((link) => link.getAttribute("href")))).toEqual(headings);
});
