import { test, expect } from "./fixtures";

const functions = "/docs/reference/vrl/functions/";

test("VRL function reference renders signatures, arguments, and example results", async ({ page }) => {
  await page.goto(functions);
  const reference = page.locator("#parse_json").locator("xpath=ancestor::div[1]");
  const heading = reference.getByRole("heading", { name: "parse_json", exact: true });
  await heading.scrollIntoViewIfNeeded();
  await expect(heading).toBeInViewport();
  await expect(reference.getByRole("link", { name: "fallible", exact: true })).toBeVisible();

  const signature = reference.locator("#parse_json-function-spec").locator("xpath=../following-sibling::div[1]");
  await expect(signature).toContainText(/parse_json\s*\(\s*value:\s*<string>/);
  await expect(signature).toContainText(/::\s*<[^>]*\bobject\b[^>]*>\s*,\s*<error>/);

  const argumentsTable = reference.getByRole("table");
  const value = argumentsTable.getByRole("row").filter({
    has: page.getByRole("cell", { name: "value", exact: true })
  });
  await expect(value.getByRole("cell", { name: "string", exact: true })).toBeVisible();
  await expect(value.getByRole("cell", { name: "yes", exact: true })).toBeVisible();
  const maxDepth = argumentsTable.getByRole("row").filter({
    has: page.getByRole("cell", { name: "max_depth", exact: true })
  });
  await expect(maxDepth.getByRole("cell", { name: "integer", exact: true })).toBeVisible();
  await expect(maxDepth.getByRole("cell", { name: "no", exact: true })).toBeVisible();

  const example = reference.getByRole("heading", { name: "Parse JSON", exact: true }).locator("xpath=ancestor::div[1]");
  await example.scrollIntoViewIfNeeded();
  await expect(example.locator("pre").first()).toHaveText('parse_json!(s\'{"key": "val"}\')');
  const result = example.locator("pre").nth(1);
  await expect(result).toBeInViewport();
  expect(JSON.parse(await result.innerText())).toEqual({ key: "val" });
});

test("console sink renders generated field metadata and configuration", async ({ page }) => {
  await page.goto("/docs/reference/configuration/sinks/console/");
  const content = page.getByRole("main", { name: "Main documentation content" });
  const inputs = content.getByRole("heading", { name: "inputs", exact: true }).locator("xpath=ancestor::div[1]");
  await inputs.scrollIntoViewIfNeeded();
  await expect(inputs.getByText("required", { exact: true })).toBeVisible();
  await expect(inputs.getByText("[string]", { exact: true })).toBeVisible();
  await expect(inputs.getByRole("link", { name: "source", exact: true })).toHaveAttribute(
    "href",
    "https://vector.dev/docs/reference/configuration/sources/"
  );

  const codec = content.getByRole("heading", { name: "encoding.codec", exact: true }).locator("xpath=ancestor::div[2]");
  await codec.scrollIntoViewIfNeeded();
  await expect(codec.getByText("required", { exact: true })).toBeVisible();
  const jsonOption = codec.getByRole("row").filter({
    has: page.getByRole("cell", { name: "json", exact: true })
  });
  await expect(jsonOption.getByRole("link", { name: "JSON", exact: true })).toHaveAttribute(
    "href",
    "https://www.json.org/"
  );

  const examples = content
    .getByRole("heading", { name: "Example configurations", exact: true })
    .locator("xpath=ancestor::div[1]");
  await examples.getByRole("button", { name: "JSON", exact: true }).click();
  const code = examples.locator("pre:visible");
  await expect(code).toContainText('"type": "console"');
  const config = JSON.parse(await code.innerText());
  expect(Object.values(config.sinks)).toEqual([
    expect.objectContaining({ type: "console", inputs: [expect.any(String)], encoding: { codec: "json" } })
  ]);
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
