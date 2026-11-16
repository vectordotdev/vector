import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test, expect } from "./fixtures";

const debuggingGuide = "/guides/developer/debugging/";

for (const { name, viewport, colorScheme } of [
  { name: "desktop light", viewport: { width: 1280, height: 900 }, colorScheme: "light" as const },
  { name: "mobile dark", viewport: { width: 390, height: 844 }, colorScheme: "dark" as const }
]) {
  test(`snippets honor their initial state and toggle with the keyboard on ${name}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.emulateMedia({ colorScheme });
    await page.goto(debuggingGuide);

    const server = page
      .locator("#page-content .highlight")
      .filter({ has: page.locator('code[data-lang="python"]') })
      .first();
    await expect(server.locator(".code-block-controls .code-sample-filename")).toHaveText("http-server.py");
    const serverCode = server.locator("pre");
    const serverToggle = server.getByRole("button", { name: "Expand", exact: true });
    await expect(serverCode).toBeHidden();
    await expect(serverToggle).toHaveAttribute("aria-expanded", "false");
    await serverToggle.focus();
    await page.keyboard.press("Enter");
    await expect(serverCode).toBeVisible();
    await expect(server.getByRole("button", { name: "Collapse", exact: true })).toHaveAttribute(
      "aria-expanded",
      "true"
    );

    const config = page
      .locator("#page-content .highlight")
      .filter({ has: page.locator('code[data-lang="yaml"]') })
      .first();
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
    await expect(serverCode).toBeHidden();
    await expect(serverToggle).toHaveAttribute("aria-expanded", "false");
    await expect(config.getByRole("button", { name: "Collapse", exact: true })).toHaveAttribute(
      "aria-expanded",
      "true"
    );
    await expect(code).toBeVisible();
  });
}

test("copying collapsed snippets includes their entire contents", async ({ context, page }) => {
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
  const server = page
    .locator("#page-content .highlight")
    .filter({ has: page.locator('code[data-lang="python"]') })
    .first();
  const serverSource = (await server.locator("pre").textContent())!.replace(/\n$/, "");
  await expect(server.locator("pre")).toBeHidden();
  await server.getByRole("button", { name: "Copy code to clipboard" }).click();
  await expect.poll(() => page.evaluate(() => sessionStorage.getItem("copied-code"))).toBe(serverSource);
  await expect(server.locator("pre")).toBeHidden();

  const config = page
    .locator("#page-content .highlight")
    .filter({ has: page.locator('code[data-lang="yaml"]') })
    .first();
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
  const config = page
    .locator("#page-content .highlight")
    .filter({ has: page.locator('code[data-lang="yaml"]') })
    .first();
  await expect(page.getByRole("button", { name: "Copy code to clipboard" })).toHaveCount(0);
  await config.getByRole("button", { name: "Collapse", exact: true }).click();
  await expect(config.locator("pre")).toBeHidden();
  await config.getByRole("button", { name: "Expand", exact: true }).click();
  await expect(config.locator("pre")).toBeVisible();
});

for (const width of [1280, 390]) {
  for (const canCopy of [true, false]) {
    test(`filename headers stay attached at ${width}px with clipboard ${canCopy}`, async ({
      context,
      page
    }, testInfo) => {
      await page.setViewportSize({ width, height: 900 });
      await page.emulateMedia({ colorScheme: width === 390 ? "dark" : "light" });
      await context.addInitScript((enabled) => {
        Object.defineProperty(navigator, "clipboard", {
          configurable: true,
          value: enabled
            ? {
                async writeText(text: string) {
                  sessionStorage.setItem("copied-code", text);
                }
              }
            : undefined
        });
      }, canCopy);
      await page.goto("/docs/reference/vrl/");
      const snippet = page.locator('#page-content .highlight[title="vector.yaml"]').first();
      const header = snippet.locator(".code-block-controls");
      await expect(header.locator(".code-sample-filename")).toHaveText("vector.yaml");
      await expect(snippet.getByRole("button", { name: /^(Collapse|Expand)$/ })).toHaveCount(0);
      const blockBox = (await snippet.boundingBox())!;
      const headerBox = (await header.boundingBox())!;
      const codeBox = (await snippet.locator("pre").boundingBox())!;
      expect(headerBox.y).toBeCloseTo(blockBox.y, 0);
      expect(codeBox.y).toBeCloseTo(headerBox.y + headerBox.height, 0);
      expect(headerBox.x + headerBox.width).toBeLessThanOrEqual(width);
      const copy = snippet.getByRole("button", { name: "Copy code to clipboard" });
      if (canCopy) {
        const source = (await snippet.locator("pre").textContent())!.replace(/\n$/, "");
        await copy.click();
        await expect.poll(() => page.evaluate(() => sessionStorage.getItem("copied-code"))).toBe(source);
      } else {
        await expect(copy).toHaveCount(0);
      }
      await snippet.screenshot({ path: testInfo.outputPath("filename-header.png") });
    });
  }
}

for (const colorScheme of ["light", "dark"] as const) {
  test(`VRL examples highlight function names and fields in ${colorScheme} mode`, async ({ page }, testInfo) => {
    await page.emulateMedia({ colorScheme });
    await page.goto("/docs/reference/vrl/");
    const snippet = page
      .locator(".highlight")
      .filter({ has: page.locator('code[data-lang="vrl"]') })
      .first();
    const code = snippet.locator("code");
    const plainColor = await code.evaluate((element) => getComputedStyle(element).color);
    await expect(
      code
        .locator(".vrl-function")
        .filter({ hasText: /^parse_json$/ })
        .first()
    ).toHaveCSS("color", "rgb(147, 197, 253)");
    await expect(
      code
        .locator(".vrl-property")
        .filter({ hasText: /^message$/ })
        .first()
    ).not.toHaveCSS("color", plainColor);
    const functionColor = await code
      .locator(".vrl-function")
      .first()
      .evaluate((element) => getComputedStyle(element).color);
    await expect(code.locator(".vrl-property").first()).not.toHaveCSS("color", functionColor);
    await snippet.screenshot({ path: testInfo.outputPath(`vrl-${colorScheme}.png`) });
  });
}

test.describe("VRL highlighting without JavaScript", () => {
  test.use({ javaScriptEnabled: false });

  test("ships semantic colors in the rendered HTML", async ({ page }) => {
    await page.goto("/docs/reference/vrl/");
    const code = page.locator('code[data-highlighter="syntect-vrl"]').first();
    await expect(code).toBeAttached();
    await expect(
      code
        .locator(".vrl-function")
        .filter({ hasText: /^parse_json$/ })
        .first()
    ).toHaveCount(1);
    await expect(
      code
        .locator(".vrl-property")
        .filter({ hasText: /^message$/ })
        .first()
    ).toHaveCSS("color", "rgb(110, 231, 183)");
  });
});

test("highlighted VRL retains its complete source when copied", async ({ context, page }) => {
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
  const markdown = readFileSync(new URL("../content/en/docs/reference/vrl/_index.md", import.meta.url), "utf8");
  const source = markdown.match(/```(?:coffee|vrl)\n([\s\S]*?)```/)![1].replace(/\n$/, "");
  await page.goto("/docs/reference/vrl/");
  const block = page
    .locator(".highlight")
    .filter({ has: page.locator('code[data-highlighter="syntect-vrl"]') })
    .first();
  expect((await block.locator("pre").textContent())!.replace(/\n$/, "")).toBe(source);
  await block.getByRole("button", { name: "Copy code to clipboard" }).click();
  await expect.poll(() => page.evaluate(() => sessionStorage.getItem("copied-code"))).toBe(source);
});

test("explicit VRL fences preserve language-like literals when rendered and copied", async ({ context, page }) => {
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
  const source = '.label = "language-coffee"\n.attr = s\'data-lang="coffee"\'\n# language-coffee';
  const directory = mkdtempSync(join(tmpdir(), "vector-vrl-fence-"));
  try {
    const layouts = join(directory, "layouts", "_default");
    const markup = join(layouts, "_markup");
    mkdirSync(markup, { recursive: true });
    mkdirSync(join(directory, "content"));
    writeFileSync(
      join(directory, "hugo.toml"),
      'baseURL = "http://localhost/"\ndisableKinds = ["home", "taxonomy", "term"]\n[markup.highlight]\nnoClasses = false\n'
    );
    writeFileSync(join(layouts, "single.html"), '<main id="page-content">{{ .Content }}</main>');
    copyFileSync(
      new URL("../layouts/_default/_markup/render-codeblock-vrl.html", import.meta.url),
      join(markup, "render-codeblock-vrl.html")
    );
    writeFileSync(
      join(directory, "content", "example.md"),
      '+++\ntitle = "VRL regression"\n+++\n```vrl {title="literal.vrl"}\n' + source + "\n```\n"
    );
    execFileSync("hugo", ["--source", directory, "--destination", join(directory, "public"), "--quiet"]);
    const rendered = readFileSync(join(directory, "public", "example", "index.html"), "utf8");

    await page.goto("/docs/reference/vrl/");
    const controlsScript = await page.locator('script[src*="/js/below."]').getAttribute("src");
    expect(controlsScript).toBeTruthy();
    await page.route("**/__tests__/vrl-source-preservation/", (route) =>
      route.fulfill({
        contentType: "text/html",
        body: rendered + `<script src="${controlsScript}"></script>`
      })
    );
    await page.goto("/__tests__/vrl-source-preservation/");
    const block = page.locator('.highlight[title="literal.vrl"]');
    await expect(block.locator("code")).toHaveAttribute("class", "language-vrl");
    await expect(block.locator("code")).toHaveAttribute("data-lang", "vrl");
    expect((await block.locator("pre").textContent())!.replace(/\n$/, "")).toBe(source);
    await block.getByRole("button", { name: "Copy code to clipboard" }).click();
    await expect.poll(() => page.evaluate(() => sessionStorage.getItem("copied-code"))).toBe(source);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
