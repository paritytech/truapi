import { expect, test } from "@playwright/test";
import { openPlaygroundInDotli, selectMethod, waitForOnline } from "./helpers";

test.describe("unary call", () => {
  test("get_account runs to completion without error", async ({ page }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(frame, "Account", "get_account");

    // Click `Run example`. The button is keyed on `data-testid` so it
    // is robust to label/glyph changes.
    await frame.locator('[data-testid="call-button"]').click();

    // Output is the example's console.log; failure would be an explicit
    // `assert` throw surfacing in the error panel. A successful run produces
    // at least one log entry and no error.
    const entries = frame.locator('[data-testid="stream-entry"]');
    await expect(entries.first()).toBeVisible({ timeout: 5_000 });
    await expect(frame.locator('[data-testid="error-display"]')).toHaveCount(0);

    const text = await entries.first().innerText();
    expect(text.trim().length).toBeGreaterThan(0);
  });
});
