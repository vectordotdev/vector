import { test, expect } from "./fixtures";

const quickstart = "/docs/setup/quickstart/";
const remap = "/docs/reference/configuration/transforms/remap/";

test("homepage renders the React diagram and links to VRL", async ({ page }) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Take control of your observability data", exact: true })
  ).toBeVisible();
  await expect(page.locator("#diagram svg")).toBeVisible();
  await page.locator('#diagram a[href="/docs/reference/vrl"]').click();
  await expect(page).toHaveURL(/\/docs\/reference\/vrl\/?$/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Vector Remap Language");
});

test("homepage globe renders land and markers with contrast and rotates", async ({ page }) => {
  await page.goto("/");
  const globe = page.locator("#globe svg");
  await globe.scrollIntoViewIfNeeded();
  await expect(globe).toBeInViewport({ ratio: 0.9 });

  const water = globe.locator("circle.globe");
  const land = globe.locator("path.land").first();
  const marker = globe.locator('circle.marker:not([fill="none"])').first();
  await expect(water).toBeVisible();
  await expect(land).toBeVisible();
  await expect(marker).toBeVisible();
  const waterFill = await water.evaluate((element) => getComputedStyle(element).fill);
  await expect(land).not.toHaveCSS("fill", waterFill);
  await expect(land).not.toHaveCSS("fill", "none");

  const initialLand = await land.getAttribute("d");
  await expect(land).not.toHaveAttribute("d", initialLand!);
});

test("theme changes the rendered colors and persists across reloads and pages", async ({ page }) => {
  await page.goto(quickstart);
  const body = page.locator("body");
  const toggle = page.getByRole("button", { name: "Toggle dark mode" });
  await expect(toggle).toBeVisible();
  const lightBackground = await body.evaluate((element) => getComputedStyle(element).backgroundColor);

  await toggle.click();
  await expect(body).not.toHaveCSS("background-color", lightBackground);
  const darkBackground = await body.evaluate((element) => getComputedStyle(element).backgroundColor);
  await page.reload();
  await expect(body).toHaveCSS("background-color", darkBackground);
  await page.goto(remap);
  await expect(body).toHaveCSS("background-color", darkBackground);
  await toggle.click();
  await expect(body).toHaveCSS("background-color", lightBackground);
});

test("documentation TOC navigates to a visible section", async ({ page }) => {
  await page.goto(quickstart);
  await page
    .getByRole("complementary", { name: "Table of contents" })
    .getByRole("link", { name: "Configure Vector", exact: true })
    .click();
  await expect(page).toHaveURL(/#configure-vector$/);
  await expect(page.getByRole("heading", { name: "Configure Vector", exact: true })).toBeInViewport();
});

test("configuration format and level tabs change the displayed example", async ({ page }) => {
  await page.goto(remap);
  const examples = page.locator("div[x-data]").filter({
    has: page.getByRole("heading", { name: "Example configurations", exact: true })
  });
  const code = examples.locator("pre:visible");
  await expect(code).toContainText("type: remap");
  await examples.getByRole("button", { name: "JSON", exact: true }).click();
  await expect(code).toContainText('"type": "remap"');
  await expect(code).not.toContainText("drop_on_abort");
  await examples.getByRole("button", { name: "Advanced", exact: true }).click();
  await expect(code).toContainText('"type": "remap"');
  await expect(code).toContainText('"drop_on_abort": true');

  await page.reload();
  await expect(code).toContainText('"type": "remap"');
  await examples.getByRole("button", { name: "TOML", exact: true }).click();
  await expect(code).toContainText('type = "remap"');
  await examples.getByRole("button", { name: "YAML", exact: true }).click();
  await expect(code).toContainText("type: remap");
});

test("exact component search supports keyboard navigation", async ({ page }) => {
  await page.goto(quickstart);
  await page.locator("#site-search").getByRole("button", { name: "Search", exact: false }).click();
  const search = page.getByRole("searchbox");
  await search.fill("remap transform");
  const options = page.locator(".aa-Panel").getByRole("option");
  await expect(options.nth(1)).toBeVisible();
  await expect(options.first().getByRole("link")).toHaveAttribute("href", remap);
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  await search.press("ArrowDown");
  await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");
  await search.press("ArrowUp");
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  await search.press("Enter");
  await expect(page).toHaveURL(new RegExp(`${remap}$`));
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Remap");
});

test("VRL search accepts function prefixes and navigates to the function", async ({ page }) => {
  await page.goto(quickstart);
  await page.locator("#site-search").getByRole("button", { name: "Search", exact: false }).click();
  await page.getByRole("searchbox").fill("parse_jso");
  const result = page.locator(".aa-Panel").getByRole("link").filter({ hasText: "parse_json" }).first();
  await expect(result).toHaveAttribute("href", "/docs/reference/vrl/functions/#parse_json");
  await result.click();
  await expect(page).toHaveURL(/\/docs\/reference\/vrl\/functions\/#parse_json$/);
  await expect(page.getByRole("heading", { name: "parse_json", exact: true })).toBeInViewport();
});

test("Pagefind returns full-text results and handles an unmatched query", async ({ page }) => {
  await page.goto(quickstart);
  await page.locator("#site-search").getByRole("button", { name: "Search", exact: false }).click();
  const search = page.getByRole("searchbox");
  await search.fill("backpressure");
  const result = page.locator(".aa-Panel").getByRole("link").first();
  await expect(result).toContainText(/backpressure/i);
  const href = await result.getAttribute("href");
  await result.click();
  await expect(page).toHaveURL(new URL(href!, page.url()).href);
  await page.locator("#site-search").getByRole("button", { name: "Search", exact: false }).click();
  await search.fill('"zzzxqvunmatchedquery"');
  await expect(page.getByText(/no results found/i)).toBeVisible();
  await search.press("Escape");
  await expect(page.locator(".aa-Panel")).toBeHidden();
});

test("mobile navigation opens, navigates, and closes the docs sidebar", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  const menu = page.locator("#mobile-menu");
  await expect(menu).toBeHidden();
  await page.getByRole("button", { name: "Open navbar dropdown menu" }).click();
  await expect(menu).toBeVisible();
  await menu.getByRole("link", { name: "Docs", exact: true }).click();
  await expect(page).toHaveURL(/\/docs\/?$/);
  await page.getByText("Sidebar", { exact: true }).click();
  const sidebar = page.getByRole("dialog").filter({
    has: page.getByRole("button", { name: "Close docs slideover panel" })
  });
  await expect(sidebar).toBeVisible();
  await sidebar.getByRole("button", { name: "Close docs slideover panel" }).click();
  await expect(sidebar).toBeHidden();
});
