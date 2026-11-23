import { fileURLToPath } from "node:url";
import { test as base, expect } from "@playwright/test";

export const test = base.extend<{ websiteChecks: void }>({
  websiteChecks: [
    async ({ context, page, baseURL }, use) => {
      const origin = new URL(baseURL!).origin;
      const errors: string[] = [];

      // The newsletter and tracking SDKs are outside this suite. Their bootstrap
      // hooks still run, but must not load forms or send analytics in tests.
      await context.addInitScript(() => {
        Object.assign(window, {
          Munchkin: { init() {} },
          MktoForms2: { loadForm() {} },
          // Makes `datadogRum.init` a no-op (the SDK defers to Synthetics). Otherwise RUM
          // flushes its batch on page exit, and WebKit reports the fetch it starts from
          // the unloading page as an "access control checks" page error.
          _DATADOG_SYNTHETICS_INJECTS_RUM: true
        });
      });

      // Keep real local assets and search requests; disable external analytics,
      // marketing scripts, and fonts so tests need no third-party services.
      await context.route("**/*", async (route) => {
        const url = new URL(route.request().url());
        if (url.origin === origin || !["http:", "https:"].includes(url.protocol)) {
          await route.continue();
        } else if (url.href === "https://cdn.jsdelivr.net/npm/@algolia/autocomplete-theme-classic") {
          // Test the installed theme version, not an unpinned CDN release.
          await route.fulfill({
            contentType: "text/css",
            path: fileURLToPath(
              new URL("../node_modules/@algolia/autocomplete-theme-classic/dist/theme.min.css", import.meta.url)
            )
          });
        } else {
          await route.fulfill({
            status: 200,
            contentType: route.request().resourceType() === "stylesheet" ? "text/css" : "application/javascript",
            body: ""
          });
        }
      });

      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      page.on("response", (response) => {
        if (new URL(response.url()).origin === origin && response.status() >= 400) {
          errors.push(`${response.status()} ${response.url()}`);
        }
      });
      page.on("requestfailed", (request) => {
        if (new URL(request.url()).origin === origin && request.failure()?.errorText !== "net::ERR_ABORTED") {
          errors.push(`${request.failure()?.errorText} ${request.url()}`);
        }
      });

      await use();
      expect(errors, "The website must load without runtime errors or missing local assets").toEqual([]);
    },
    { auto: true }
  ]
});

export { expect };
