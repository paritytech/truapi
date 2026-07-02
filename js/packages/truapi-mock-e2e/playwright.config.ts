import { defineConfig } from "@playwright/test";

// Local-mode browser E2E: the product runs inside the mock host (the real
// truapi-server WASM core in a Worker), driven headless. The dev server serves
// the pre-built workspace packages; build them first (see README / CI).
export default defineConfig({
  testDir: "./tests",
  timeout: 60_000,
  expect: { timeout: 40_000 },
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:4319",
    headless: true,
  },
  webServer: {
    command: "npm run dev",
    url: "http://localhost:4319/host.html",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
