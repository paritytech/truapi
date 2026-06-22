import { expect, test } from "@playwright/test";
import { openPlaygroundInDotli, selectMethod, waitForOnline } from "./helpers";

test.describe("subscription", () => {
  test("connection_status delivers an event and completes", async ({
    page,
  }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(frame, "Account", "connection_status_subscribe");

    await frame.locator('[data-testid="subscribe-button"]').click();

    // The example awaits the first event via `firstValueFrom`, logs it, and
    // completes. Connection status emits at least once on subscribe, so a
    // stream entry appears and the run finishes without an error.
    const entries = frame.locator('[data-testid="stream-entry"]');
    await expect(entries.first()).toBeVisible({ timeout: 6_000 });
    await expect(frame.locator('[data-testid="error-display"]')).toHaveCount(0);

    // Once the first event is delivered the run completes and the Run button
    // returns (no long-lived stream to Stop).
    await expect(
      frame.locator('[data-testid="subscribe-button"]'),
    ).toBeVisible();
  });
});
