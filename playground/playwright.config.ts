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
      // dotli host iframe shell at :5173. `bun run preview` runs
      // `turbo run build && bun scripts/preview-server.ts`, so a cold
      // CI runner needs the long timeout.
      command: "bun run preview",
      cwd: "../hosts/dotli",
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
