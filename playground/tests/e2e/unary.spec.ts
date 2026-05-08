import { expect, test } from "@playwright/test";
import {
  openPlaygroundInDotli,
  selectMethod,
  waitForOnline,
} from "./helpers";

test.describe("unary call", () => {
  test("host_account_get returns a successful response", async ({ page }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(frame, "Account Management", "host_account_get");

    // Click `Call method`. The button is keyed on `data-testid` so it
    // is robust to label/glyph changes.
    await frame.locator('[data-testid="call-button"]').click();

    // The response panel only mounts when there is a response or error.
    await expect(frame.locator('[data-testid="response-content"]')).toBeVisible(
      { timeout: 5_000 },
    );

    // Sanity: no error displayed and the response is non-empty.
    await expect(frame.locator('[data-testid="error-display"]')).toHaveCount(0);
    const responseText = await frame
      .locator('[data-testid="response-content"]')
      .innerText();
    expect(responseText.trim().length).toBeGreaterThan(0);
  });
});
