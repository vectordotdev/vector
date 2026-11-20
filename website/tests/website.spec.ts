import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures";

const quickstart = "/docs/setup/quickstart/";
const remap = "/docs/reference/configuration/transforms/remap/";

test("homepage renders the React globe and diagram and links to VRL", async ({ page }) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Take control of your observability data", exact: true })
  ).toBeVisible();
  await expect(page.locator("#globe svg")).toBeVisible();
  await expect(page.locator("#diagram svg")).toBeVisible();
  await page.locator('#diagram a[href="/docs/reference/vrl"]').click();
  await expect(page).toHaveURL(/\/docs\/reference\/vrl\/?$/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Vector Remap Language");
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

test("VRL search accepts function prefixes and navigates to the function", async ({ page, browserName }) => {
  if (browserName === "firefox") {
    // Reproduce Firefox scrolling to the fragment before DOM-ready controls change its position.
    await page.addInitScript(() => {
      document.addEventListener("DOMContentLoaded", () => document.querySelector(":target")?.scrollIntoView(), {
        capture: true
      });
      window.addEventListener("DOMContentLoaded", () => {
        const target = document.querySelector(":target");
        if (!target) return;
        const rect = target.getBoundingClientRect();
        sessionStorage.setItem("fragment-in-view-after-init", String(rect.top < innerHeight && rect.bottom > 0));
      });
    });
  }
  await page.goto(quickstart);
  await page.locator("#site-search").getByRole("button", { name: "Search", exact: false }).click();
  await page.getByRole("searchbox").fill("parse_jso");
  const result = page.locator(".aa-Panel").getByRole("link").filter({ hasText: "parse_json" }).first();
  await expect(result).toHaveAttribute("href", "/docs/reference/vrl/functions/#parse_json");
  await result.click();
  // The click resolves at commit; WebKit on CI can still be parsing this large page, so wait for load.
  await page.waitForURL(/\/docs\/reference\/vrl\/functions\/#parse_json$/);
  if (browserName === "firefox") {
    expect(await page.evaluate(() => sessionStorage.getItem("fragment-in-view-after-init"))).toBe("true");
  }
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

async function checkSidebarNavigation(page: Page, dismissWith: "button" | "Escape") {
  const sidebarToggle = page.getByText("Sidebar", { exact: true });
  const sidebar = page.getByRole("dialog").filter({
    has: page.getByRole("button", { name: "Close docs slideover panel" })
  });
  await sidebarToggle.click();
  await expect(sidebar).toBeVisible();
  await expect(sidebarToggle).toBeHidden();
  if (dismissWith === "Escape") {
    await page.keyboard.press("Escape");
  } else {
    await sidebar.getByRole("button", { name: "Close docs slideover panel" }).click();
  }
  await expect(sidebar).toBeHidden();
  await expect(sidebarToggle).toBeVisible();
  await sidebarToggle.click();
  await sidebar.getByRole("link", { name: "Vector Remap Language", exact: true }).click();
  await expect(page).toHaveURL(/\/docs\/reference\/vrl\/?$/);
}

test("mobile navigation opens, navigates, and closes the docs sidebar", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  const menu = page.locator("#mobile-menu");
  await expect(menu).toBeHidden();
  await page.getByRole("button", { name: "Open navbar dropdown menu" }).click();
  await expect(menu).toBeVisible();
  await menu.getByRole("link", { name: "Docs", exact: true }).click();
  await expect(page).toHaveURL(/\/docs\/?$/);
  await checkSidebarNavigation(page, "button");
});

test("mobile component sidebar opens, closes, and navigates", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/docs/reference/configuration/sinks/amqp/");
  await checkSidebarNavigation(page, "Escape");
});

test("mobile Updates modal covers the sidebar control and dismisses from the bottom backdrop", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/docs/");
  const sidebarToggle = page.getByText("Sidebar", { exact: true });
  const sidebarControl = (await sidebarToggle.boundingBox())!;
  const sidebar = page.getByRole("dialog").filter({
    has: page.getByRole("button", { name: "Close docs slideover panel" })
  });
  const modal = page.locator('[id="12461"]');
  await page.getByRole("button", { name: "Open navbar dropdown menu" }).click();
  await page.locator("#mobile-menu").getByRole("button", { name: "Updates", exact: true }).click();
  await expect(modal).toBeVisible();

  // Tap the backdrop at the underlying Sidebar control, not the modal close button.
  await page.mouse.click(sidebarControl.x + sidebarControl.width / 2, sidebarControl.y + sidebarControl.height / 2);
  await expect(modal).toBeHidden();
  await expect(sidebar).toBeHidden();
  await sidebarToggle.click();
  await expect(sidebar).toBeVisible();
});
