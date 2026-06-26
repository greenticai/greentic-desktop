import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    actionTimeout: 15_000,
    navigationTimeout: 20_000,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium-smoke",
      use: { ...devices["Desktop Chrome"] },
      grep: /@smoke/,
    },
    {
      name: "chromium-functional",
      use: { ...devices["Desktop Chrome"] },
      grepInvert: /@smoke|@manual|@desktop-real|@java-real/,
    },
    {
      name: "desktop-real-macos",
      use: { ...devices["Desktop Chrome"] },
      grep: /@manual|@desktop-real|@java-real/,
    },
    {
      name: "desktop-real-windows",
      use: { ...devices["Desktop Chrome"] },
      grep: /@manual|@desktop-real|@java-real/,
    },
    {
      name: "desktop-real-linux",
      use: { ...devices["Desktop Chrome"] },
      grep: /@manual|@desktop-real|@java-real/,
    },
  ],
});
