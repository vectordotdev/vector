import { test, expect } from "./fixtures";

const debuggingGuide = "/guides/developer/debugging/";

for (const { name, viewport, colorScheme } of [
  { name: "desktop light", viewport: { width: 1280, height: 900 }, colorScheme: "light" as const },
  { name: "mobile dark", viewport: { width: 390, height: 844 }, colorScheme: "dark" as const }
]) {
  test(`long snippets start expanded and toggle with the keyboard on ${name}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.emulateMedia({ colorScheme });
    await page.goto(debuggingGuide);

    const config = page.locator("#page-content .highlight").first();
    const code = config.locator("pre");
    const toggle = config.getByRole("button", { name: "Collapse", exact: true });
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(code).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-controls", (await code.getAttribute("id"))!);
    const expandedHeight = (await config.boundingBox())!.height;

    await toggle.focus();
    await page.keyboard.press("Enter");
    const expand = config.getByRole("button", { name: "Expand", exact: true });
    await expect(expand).toHaveAttribute("aria-expanded", "false");
    await expect(expand).toBeFocused();
    await expect(code).toBeHidden();
    expect((await config.boundingBox())!.height).toBeLessThan(expandedHeight / 2);
    const controls = await config.locator(".code-block-controls").boundingBox();
    expect(controls!.x).toBeGreaterThanOrEqual(0);
    expect(controls!.x + controls!.width).toBeLessThanOrEqual(viewport.width);

    await page.keyboard.press("Space");
    await expect(code).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(toggle).toBeFocused();

    const shortCommand = page.locator("#page-content .highlight").filter({
      hasText: "vector tap --outputs-of source_0,transform_0"
    });
    await expect(shortCommand.locator("pre")).toBeVisible();
    await expect(shortCommand.getByRole("button", { name: /^(Collapse|Expand)$/ })).toHaveCount(0);

    await toggle.click();
    await page.reload();
    await expect(config.getByRole("button", { name: "Collapse", exact: true })).toHaveAttribute(
      "aria-expanded",
      "true"
    );
    await expect(code).toBeVisible();
  });
}

test("copying a collapsed snippet includes the entire configuration", async ({ context, page }) => {
  await context.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        async writeText(text: string) {
          sessionStorage.setItem("copied-code", text);
        }
      }
    });
  });
  await page.goto(debuggingGuide);
  const config = page.locator("#page-content .highlight").first();
  const original = (await config.locator("pre").textContent())!.replace(/\n$/, "");
  await config.getByRole("button", { name: "Collapse", exact: true }).click();
  await config.getByRole("button", { name: "Copy code to clipboard" }).click();
  await expect.poll(() => page.evaluate(() => sessionStorage.getItem("copied-code"))).toBe(original);
  await expect(config.locator("pre")).toBeHidden();
});

test("long snippets still toggle when the Clipboard API is unavailable", async ({ context, page }) => {
  await context.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
  });
  await page.goto(debuggingGuide);
  const config = page.locator("#page-content .highlight").first();
  await expect(page.getByRole("button", { name: "Copy code to clipboard" })).toHaveCount(0);
  await config.getByRole("button", { name: "Collapse", exact: true }).click();
  await expect(config.locator("pre")).toBeHidden();
  await config.getByRole("button", { name: "Expand", exact: true }).click();
  await expect(config.locator("pre")).toBeVisible();
});
