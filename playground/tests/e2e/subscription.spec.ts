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

    await expect(
      frame
        .locator('[data-testid="stream-entry"]')
        .filter({ hasText: "--- stopped ---" }),
    ).toHaveCount(1);
    await expect(
      frame
        .locator('[data-testid="stream-entry"]')
        .filter({ hasText: "--- stream ended ---" }),
    ).toHaveCount(0);

    // After stopping, the subscribe button comes back and the UI records a
    // local stop instead of synthesizing a normal stream completion.
    await expect(
      frame.locator('[data-testid="subscribe-button"]'),
    ).toBeVisible();
    await expect(
      frame.locator('[data-testid="error-display"]'),
    ).toHaveCount(0);
  });
});
