import { describe, expect, it } from "bun:test";

import {
  dispatchChainResponse,
  dispatchSubscriptionItem,
} from "./worker-dispatch.js";
import type { WorkerToMain } from "./worker-protocol.js";

describe("worker dispatch guards", () => {
  it("stops a subscription when its WASM listener throws", () => {
    const messages: WorkerToMain[] = [];
    const listeners = new Map<number, (value: unknown) => void>([
      [
        7,
        () => {
          throw new Error("panic");
        },
      ],
    ]);

    expect(() =>
      dispatchSubscriptionItem(7, "item", listeners, (msg) =>
        messages.push(msg),
      ),
    ).not.toThrow();

    expect(listeners.has(7)).toBe(false);
    expect(messages).toEqual([
      { kind: "subscriptionStop", subId: 7 },
      { kind: "disposeError", error: "subscription 7 callback failed: panic" },
    ]);
  });

  it("closes a chain connection when its WASM listener throws", () => {
    const messages: WorkerToMain[] = [];
    const listeners = new Map<number, (json: string) => void>([
      [
        11,
        () => {
          throw new Error("panic");
        },
      ],
    ]);

    expect(() =>
      dispatchChainResponse(11, "{}", listeners, (msg) => messages.push(msg)),
    ).not.toThrow();

    expect(listeners.has(11)).toBe(false);
    expect(messages).toEqual([
      { kind: "chainClose", connId: 11 },
      {
        kind: "disposeError",
        error: "chain connection 11 callback failed: panic",
      },
    ]);
  });
});
