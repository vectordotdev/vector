import { readFileSync } from "node:fs";
import { test, expect } from "./fixtures";

const functions = "/docs/reference/vrl/functions/";

function consoleExample(level: "minimal" | "advanced") {
  const file = new URL(`../generated/example-configs/sinks/console/${level}.yaml`, import.meta.url);
  return readFileSync(file, "utf8").trimEnd();
}

test("generated VRL function and component references render", async ({ page }) => {
  await page.goto(functions);
  await expect(page.getByRole("heading", { name: "parse_json", exact: true })).toBeVisible();

  await page.goto("/docs/reference/configuration/sinks/console/");
  const content = page.getByRole("main", { name: "Main documentation content" });
  await expect(content.getByRole("heading", { name: "inputs", exact: true })).toBeVisible();
  const examples = content.locator("div[x-data]").filter({
    has: page.getByRole("heading", { name: "Example configurations", exact: true })
  });
  const code = async () => (await examples.locator("pre:visible").innerText()).trimEnd();
  await expect.poll(code).toBe(consoleExample("minimal"));
  await examples.getByRole("button", { name: "Advanced", exact: true }).click();
  await expect.poll(code).toBe(consoleExample("advanced"));
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

for (const { sink, usesUri } of [
  { sink: "vector", usesUri: false },
  { sink: "http", usesUri: true }
]) {
  test(`${sink} healthcheck documents both input forms and applicable object fields`, async ({ page }) => {
    await page.goto(`/docs/reference/configuration/sinks/${sink}/`);
    const heading = page.getByRole("heading", { name: "healthcheck", exact: true });
    const header = heading.locator("../..");
    await expect(header.getByText("bool", { exact: true })).toBeVisible();
    await expect(header.getByText("object", { exact: true })).toBeVisible();
    const option = header.locator("..");
    await expect(option.getByText(/Boolean shorthand is deprecated/)).toBeVisible();
    await expect(option.getByRole("link", { name: "migration guide", exact: true })).toHaveAttribute(
      "href",
      "/deprecations/"
    );
    await expect(page.getByRole("heading", { name: "healthcheck.enabled", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "healthcheck.uri", exact: true })).toHaveCount(usesUri ? 1 : 0);
    await expect(
      page.getByText("A boolean value sets enabled; other healthcheck options retain their defaults.", { exact: true })
    ).toBeVisible();
    await option.getByRole("link", { name: "migration guide", exact: true }).click();
    await expect(
      page.getByText("Boolean shorthand for object-valued configuration options", { exact: true })
    ).toBeVisible();
    await expect(page.getByText(/We will allow at least two minor release cycles/)).toBeVisible();
  });
}
