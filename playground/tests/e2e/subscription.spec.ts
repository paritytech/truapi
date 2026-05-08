import { expect, test } from "@playwright/test";
import {
  openPlaygroundInDotli,
  selectMethod,
  waitForOnline,
} from "./helpers";

test.describe("subscription", () => {
  test("connection_status pushes events and stops cleanly", async ({
    page,
  }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(
      frame,
      "Account Management",
      "host_account_connection_status_subscribe",
    );

    await frame.locator('[data-testid="subscribe-button"]').click();

    // Connection status emits at least once on subscribe; assert at
    // least one stream entry within the SUBSCRIPTION_TIMEOUT_MS budget
    // used by the playground's auto-test runner (6s).
    const streamEntries = frame.locator('[data-testid="stream-entry"]');
    await expect(streamEntries.first()).toBeVisible({ timeout: 6_000 });

    // The subscribe button has been replaced by the stop button.
    const stopButton = frame.locator('[data-testid="stop-button"]');
    await expect(stopButton).toBeVisible();
    await stopButton.click();

    // After stopping, the subscribe button comes back. This proves the
    // _stop frame round-tripped, not just that the UI was reset.
    await expect(
      frame.locator('[data-testid="subscribe-button"]'),
    ).toBeVisible();
    await expect(
      frame.locator('[data-testid="error-display"]'),
    ).toHaveCount(0);
  });
});
