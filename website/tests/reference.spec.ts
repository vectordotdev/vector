import { readFileSync } from "node:fs";
import { test, expect } from "./fixtures";

const functions = "/docs/reference/vrl/functions/";

function consoleExample(level: "minimal" | "advanced") {
  const file = new URL(`../generated/example-configs/sinks/console/${level}.yaml`, import.meta.url);
  return readFileSync(file, "utf8").trimEnd();
}

test("generated component examples switch formats and levels and persist the format", async ({ page }) => {
  await page.goto("/docs/reference/configuration/sinks/console/");
  const content = page.getByRole("main", { name: "Main documentation content" });
  await expect(content.getByRole("heading", { name: "inputs", exact: true })).toBeVisible();
  const examples = content.locator("div[x-data]").filter({
    has: page.getByRole("heading", { name: "Example configurations", exact: true })
  });
  const code = async () => (await examples.locator("pre:visible").innerText()).trimEnd();
  await expect.poll(code).toBe(consoleExample("minimal"));
  await examples.getByRole("button", { name: "JSON", exact: true }).click();
  await expect.poll(code).toContain('"type": "console"');
  await expect.poll(code).not.toContain('"target"');
  await examples.getByRole("button", { name: "Advanced", exact: true }).click();
  await expect.poll(code).toContain('"target": "stdout"');
  await examples.getByRole("button", { name: "YAML", exact: true }).click();
  await expect.poll(code).toBe(consoleExample("advanced"));

  await examples.getByRole("button", { name: "JSON", exact: true }).click();
  await page.reload();
  await expect.poll(code).toContain('"type": "console"');
  await examples.getByRole("button", { name: "TOML", exact: true }).click();
  await expect.poll(code).toContain('type = "console"');
  await examples.getByRole("button", { name: "YAML", exact: true }).click();
  await expect.poll(code).toBe(consoleExample("minimal"));
});

test("map-typed config fields render their examples", async ({ page }) => {
  await page.goto("/docs/reference/configuration/sources/kafka/");
  const field = page.locator('[id="librdkafka_options"]').locator("xpath=ancestor::div[contains(@class, 'p-4')][1]");
  await expect(field.getByRole("heading", { name: "librdkafka_options", exact: true })).toBeVisible();

  const examples = field.locator("div[x-data]").filter({ hasText: "Examples" });
  await expect(examples).toHaveCount(1);
  const code = async () => (await examples.locator("pre:visible").innerText()).trimEnd();
  await examples.locator("a").click();
  await expect.poll(code).toContain('"client.id": "${ENV_VAR}"');
});

test("VRL TOC highlight follows scrolling between functions and their examples", async ({ page }) => {
  await page.goto(functions);
  const toc = page.locator("#toc");
  const jsonLink = toc.getByRole("link", { name: "parse_json", exact: true });
  await expect(jsonLink).toBeAttached();
  const inactiveColor = await jsonLink.evaluate((element) => getComputedStyle(element).color);

  // Scroll the document, not the TOC links: move forwards, then backwards into
  // an example whose own heading must not replace the function's highlight.
  for (const [headingId, functionId] of [
    ["parse_json", "parse_json"],
    ["parse_syslog", "parse_syslog"],
    ["parse_json-examples-parse-json", "parse_json"]
  ]) {
    await page.locator(`[id="${headingId}"]`).evaluate((element) => {
      window.scrollTo({ top: window.scrollY + element.getBoundingClientRect().top - 40, behavior: "instant" });
    });
    const active = toc.locator("a.is-active-link");
    await expect(active).toHaveAttribute("href", `#${functionId}`);
    await expect(active).toHaveCSS("font-weight", "700");
    await expect(active).not.toHaveCSS("color", inactiveColor);
    await active.scrollIntoViewIfNeeded();
    await expect(active).toBeInViewport();
  }
  await expect(page).toHaveURL(new RegExp(`${functions}$`));
});
