import { test, expect } from "@playwright/test";

// End-to-end, in a real browser: the HOST page boots createMockHost + the real
// truapi-server WASM core in a Web Worker, and embeds the PRODUCT in an iframe.
// The product uses the SDK's normal sandbox path (getClientSync) and makes calls
// that flow — over a real MessageChannel — through the real dispatcher to the
// mock. This is "local test mode": the product tests itself, no device.
test.describe("TrUAPI mock host — browser E2E (product-in-iframe, real WASM core)", () => {
  test("product calls round-trip through the real core to the mock host", async ({ page }) => {
    await page.goto("/host.html");

    // Host boots the WASM core in a Worker and embeds the product iframe.
    await expect(page.getByTestId("host-status")).toContainText("ready");

    // The product (in the iframe) connects via the SDK sandbox and runs its calls.
    const product = page.frameLocator('[data-testid="frame"] iframe');
    await expect(product.getByTestId("status")).toContainText("done");

    // Each product call, asserted individually (the "run each in local mode" loop):
    await expect(product.getByTestId("row-localStorage round-trip")).toHaveText('"nidish"');
    await expect(product.getByTestId("row-permissions.Camera (allow-all)")).toHaveText("granted=true");
    await expect(product.getByTestId("row-system.featureSupported(Chain)")).toHaveText("supported=true");
    await expect(product.getByTestId("row-system.navigateTo")).toContainText("sent");

    // Host-side assertion surface: navigateTo actually reached the mock platform.
    const navigations = await page.evaluate(() => window.__MOCK_HOST__?.navigations?.());
    expect(navigations).toEqual(["https://polkadot.network/"]);
  });
});

// Single-page topology (`/` → src/main.tsx): the whole host+product setup is a
// single `createMockClient()` call. This is the executing coverage for that
// one-liner export, distinct from the hand-wired iframe topology above.
test.describe("TrUAPI mock host — createMockClient one-liner (single page, real WASM core)", () => {
  test("createMockClient drives a product client through the real core to the mock host", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("status")).toContainText("done");

    await expect(page.getByTestId("row-localStorage round-trip")).toHaveText('"nidish"');
    await expect(page.getByTestId("row-permissions.Camera (allow-all)")).toHaveText("granted=true");
    await expect(page.getByTestId("row-system.featureSupported(Chain)")).toHaveText("supported=true");
    // main.tsx records mock.navigations() into this row — proving the call reached the mock.
    await expect(page.getByTestId("row-system.navigateTo → host recorded")).toContainText("polkadot.network");
  });
});
