import { expect, test } from "@playwright/test";
import { openPlaygroundInDotli, waitForOnline } from "./helpers";

test.describe("handshake", () => {
  test("connection chip flips to Host Linked", async ({ page }) => {
    const frame = await openPlaygroundInDotli(page);

    // The chip should not stay on `connecting` indefinitely: the
    // host_handshake_request → host_handshake_response round trip must
    // complete and flip the chip into the connected state.
    await waitForOnline(frame);

    // Once connected, the splash unmounts and the service rail mounts.
    // The Auto-Test entry button is the simplest stable proof of that.
    await expect(
      frame.getByRole("button", { name: /Auto-Test Run all methods/ }),
    ).toBeVisible();
  });
});
