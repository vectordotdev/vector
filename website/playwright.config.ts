import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  workers: process.env.CI === "true" ? 1 : 3,
  reporter: [
    ["list"],
    ["html", { open: "never" }],
    ["junit", { outputFile: "test-results/junit.xml", includeProjectInTestName: true }]
  ],
  use: {
    baseURL: "http://127.0.0.1:4173",
    colorScheme: "light",
    trace: "retain-on-failure",
    screenshot: "only-on-failure"
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } }
  ],
  webServer: {
    command: "python3 -m http.server 4173 --bind 127.0.0.1 --directory public",
    url: "http://127.0.0.1:4173",
    stderr: "ignore",
    reuseExistingServer: false
  }
});
