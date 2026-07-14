// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import type { Page } from "@playwright/test";
import jsQR from "jsqr";

export async function extractQrPayload(
  page: Page,
  canvasSelector: string,
  timeoutMs = 30_000,
): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const embedded = await page
      .locator(canvasSelector)
      .getAttribute("data-qr-payload", { timeout: 250 })
      .catch(() => null);
    if (embedded?.startsWith("polkadotapp://")) {
      return embedded;
    }

    const px = await page.evaluate((sel) => {
      const canvas = document.querySelector(sel) as HTMLCanvasElement | null;
      if (!canvas || canvas.width === 0) return null;
      const ctx = canvas.getContext("2d");
      if (!ctx) return null;
      const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
      return {
        data: Array.from(img.data),
        width: img.width,
        height: img.height,
      };
    }, canvasSelector);

    if (px) {
      const code = jsQR(new Uint8ClampedArray(px.data), px.width, px.height);
      if (code?.data?.startsWith("polkadotapp://")) {
        return code.data;
      }
    }
    await page.waitForTimeout(1_000);
  }
  throw new Error("Could not decode QR payload from canvas");
}
