import { expect, test, type Page } from "@playwright/test";
import { openPlaygroundInDotli, waitForOnline } from "./helpers";

/**
 * The login pairing UI is driven by the Rust core's ordered auth-state
 * stream. This spec pins the two failure modes of the event-soup era:
 * a boot-time disconnected tick closing the just-opened pairing modal,
 * and a dismissed modal leaving the login flow polling forever.
 */
test.describe("login pairing modal", () => {
  test("stays open while pairing, cancels on close, reopens on retry", async ({
    page,
  }) => {
    const subscribeSends: number[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (
        text.includes("chainSend") &&
        text.includes("statement_subscribeStatement")
      ) {
        subscribeSends.push(Date.now());
      }
    });
    await page.addInitScript(() => {
      try {
        localStorage.setItem("truapi:logLevel", "debug");
      } catch {
        /* storage unavailable */
      }
    });

    const frame = await openPlaygroundInDotli(page);
    await waitForOnline(frame);

    // Opening the login modal renders the pairing QR from the core's
    // `Pairing` auth state.
    await openPairingModal(page);

    // The modal must survive the first seconds of pairing: the freshly
    // booted core's session-store sync must not tear it down.
    await page.waitForTimeout(5_000);
    await expect(page.locator("#auth-modal-backdrop.open")).toBeVisible();
    await expect(page.locator("#auth-modal-qr canvas")).toBeVisible();

    // While pairing, the core polls the statement store with ~2s
    // snapshot queries.
    expect(subscribeSends.length).toBeGreaterThanOrEqual(2);

    // Closing the modal cancels the login in the core: polling stops.
    await page.locator("#auth-modal-close").click();
    await expect(page.locator("#auth-modal-backdrop.open")).toBeHidden();
    await page.waitForTimeout(1_000); // grace for an in-flight tick
    const sendsAtCancel = subscribeSends.length;
    await page.waitForTimeout(6_000);
    expect(subscribeSends.length).toBe(sendsAtCancel);

    // Retry opens a fresh pairing modal.
    await openPairingModal(page);
  });
});

async function openPairingModal(page: Page): Promise<void> {
  await page.locator("#auth-button").click();
  await expect(page.locator("#auth-modal-backdrop.open")).toBeVisible();
  await expect(page.locator("#auth-modal-qr canvas")).toBeVisible({
    timeout: 15_000,
  });
}
