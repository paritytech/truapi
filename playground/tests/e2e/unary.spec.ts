import { expect, test } from "@playwright/test";
import { openPlaygroundInDotli, selectMethod, waitForOnline } from "./helpers";

test.describe("unary call", () => {
  test("handshake returns a successful response", async ({ page }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(frame, "System", "handshake");

    // Click `Call method`. The button is keyed on `data-testid` so it
    // is robust to label/glyph changes.
    await frame.locator('[data-testid="call-button"]').click();

    // The handshake method returns `undefined`, so the playground records a
    // successful `ok` entry instead of rendering a structured response body.
    await expect(
      frame.locator('[data-testid="stream-entry"]').filter({ hasText: "ok" }),
    ).toHaveCount(1, { timeout: 5_000 });

    // Sanity: no error displayed.
    await expect(frame.locator('[data-testid="error-display"]')).toHaveCount(0);
  });
});
