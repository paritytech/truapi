// Single source of truth for streaming host subscriptions crossing the
// worker boundary. Both the main-thread provider (callback dispatch and
// `optionalSubscriptions` advertisement) and the worker runtime (raw
// callback construction) derive their behavior from this table.

import type { WasmRawCallbacks } from "./runtime.js";
import type { SubscriptionName } from "./worker-protocol.js";

/** Host-side callback set the subscription adapters dispatch into. */
export type SubscriptionCallbacks = Omit<WasmRawCallbacks, "emitFrame">;

/** Pushes one subscription item back to the worker. */
export type PushItem = (value?: unknown) => void;

type StartResult = (() => void) | void;

/**
 * One streaming subscription: the `WasmRawCallbacks` key implementing it,
 * its wire-protocol name, and a `start` adapter typed per entry. Entries
 * with `payload: "required"` are only started with a non-null payload.
 */
export type SubscriptionDispatchEntry = {
  readonly callback: keyof SubscriptionCallbacks;
  readonly protocol: SubscriptionName;
} & (
  | {
      readonly payload: "none";
      readonly start: (
        callbacks: SubscriptionCallbacks,
        push: PushItem,
      ) => StartResult;
    }
  | {
      readonly payload: "required";
      readonly start: (
        callbacks: SubscriptionCallbacks,
        payload: Uint8Array,
        push: PushItem,
      ) => StartResult;
    }
);

/** Every streaming subscription the worker bridge knows how to dispatch. */
export const SUBSCRIPTION_DISPATCH: readonly SubscriptionDispatchEntry[] = [
  {
    callback: "subscribeSessionStore",
    protocol: "sessionStoreSubscribe",
    payload: "none",
    start: (callbacks, push) => callbacks.subscribeSessionStore?.(push),
  },
  {
    callback: "themeSubscribe",
    protocol: "themeSubscribe",
    payload: "none",
    start: (callbacks, push) => callbacks.themeSubscribe?.(push),
  },
  {
    callback: "preimageLookupSubscribe",
    protocol: "preimageLookupSubscribe",
    payload: "required",
    start: (callbacks, payload, push) =>
      callbacks.preimageLookupSubscribe(payload, push),
  },
];

/** Looks up the dispatch entry for a wire subscription name, if known. */
export function subscriptionDispatchEntry(
  name: SubscriptionName,
): SubscriptionDispatchEntry | undefined {
  return SUBSCRIPTION_DISPATCH.find((entry) => entry.protocol === name);
}
