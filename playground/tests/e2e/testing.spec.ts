import { expect, test } from "@playwright/test";
import { openPlaygroundInDotli, selectMethod, waitForOnline } from "./helpers";

test.describe("testing service", () => {
  test("probe runs through the latest generated request version", async ({
    page,
  }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(frame, "Testing", "probe");
    await frame.locator('[data-testid="call-button"]').click();

    const entries = frame.locator('[data-testid="stream-entry"]');
    await expect(entries.first()).toBeVisible({ timeout: 5_000 });
    await expect(frame.locator('[data-testid="error-display"]')).toHaveCount(0);

    const text = await entries.first().innerText();
    expect(text).toContain("testing probe:");
    expect(text).toContain("receivedVersion");
    expect(text).toContain("2");
  });

  test("framework_error surfaces a framework call error", async ({ page }) => {
    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    await selectMethod(frame, "Testing", "framework_error");
    await frame.locator('[data-testid="call-button"]').click();

    const entries = frame.locator('[data-testid="stream-entry"]');
    await expect(entries.first()).toBeVisible({ timeout: 5_000 });
    await expect(frame.locator('[data-testid="error-display"]')).toHaveCount(0);

    const text = await entries.first().innerText();
    expect(text).toContain("framework error:");
    expect(text).toContain("HostFailure");
    expect(text).toContain("forced by testing.framework_error");
  });
});
