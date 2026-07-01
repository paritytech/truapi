import { defineConfig, devices } from "@playwright/test";

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  forbidOnly: isCI,
  retries: isCI ? 1 : 0,
  workers: 1,
  reporter: isCI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:5173",
    serviceWorkers: "block",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: [
    {
      // dotli host iframe shell at :5173. Localhost product proxy routes are
      // debug-build-only, so mirror `make dev` and run the debug preview.
      command: "bun run preview:debug",
      cwd: "../hosts/dotli",
      env: {
        VITE_NETWORKS: process.env.VITE_NETWORKS ?? "paseo-next-v2,previewnet",
      },
      url: "http://localhost:5173",
      reuseExistingServer: !isCI,
      timeout: 10 * 60 * 1000,
      stdout: "pipe",
      stderr: "pipe",
    },
    {
      command: "yarn dev",
      url: "http://localhost:3000",
      reuseExistingServer: !isCI,
      timeout: 2 * 60 * 1000,
      stdout: "pipe",
      stderr: "pipe",
    },
  ],
});
