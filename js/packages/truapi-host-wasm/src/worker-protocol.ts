// Wire format between the main thread (`createWebWorkerProvider`) and the
// Web Worker that hosts the truapi-server WASM core.
//
// Frames (`kind: 'frame'`) carry SCALE-encoded `ProtocolMessage` bytes
// untouched in either direction. Everything else is a control message
// for callback dispatch, subscription bookkeeping, or chain connections.
//
// Frame bytes cross the boundary by structured clone, deliberately not as
// transferables: the sender keeps using its buffer (the worker side posts
// views into WASM memory) and frames are small, so the copy is the simpler
// safe choice.

import type { LogLevel } from "./runtime.js";

/**
 * Names of every request/response style host callback the wasm core can
 * invoke. Names match the camelCase property keys of `WasmRawCallbacks`.
 */
export const CALLBACK_NAMES = [
  "navigateTo",
  "pushNotification",
  "cancelNotification",
  "devicePermission",
  "remotePermission",
  "featureSupported",
  "localStorageRead",
  "localStorageWrite",
  "localStorageClear",
  "presentPairing",
  "readSession",
  "writeSession",
  "clearSession",
  "sessionUiChanged",
  "confirmSignPayload",
  "confirmSignRaw",
  "confirmCreateTransaction",
  "confirmAccountAlias",
  "confirmResourceAllocation",
  "confirmPreimageSubmit",
  "submitPreimage",
] as const;

export type CallbackName = (typeof CALLBACK_NAMES)[number];

export const OPTIONAL_CALLBACK_NAMES = [
  "cancelNotification",
  "presentPairing",
  "readSession",
  "writeSession",
  "clearSession",
  "sessionUiChanged",
  "confirmSignPayload",
  "confirmSignRaw",
  "confirmCreateTransaction",
  "confirmAccountAlias",
  "confirmResourceAllocation",
  "confirmPreimageSubmit",
  "submitPreimage",
] as const satisfies readonly CallbackName[];

export type OptionalCallbackName = (typeof OPTIONAL_CALLBACK_NAMES)[number];

/**
 * Names of every subscription host callback. Each has the shape
 * `(payload?, sendItem) => dispose | void`.
 */
export type SubscriptionName =
  | "sessionStoreSubscribe"
  | "preimageLookupSubscribe"
  | "themeSubscribe";

/**
 * Positional arguments for a callback. The wasm core calls each callback
 * at a fixed arity; a uniform `unknown[]` keeps the wire protocol simple.
 */
export type CallbackArgs = readonly unknown[];

export type MainToWorker =
  | {
      kind: "init";
      logLevel: LogLevel;
      runtimeConfig: unknown;
      optionalCallbacks?: readonly OptionalCallbackName[];
      optionalSubscriptions?: readonly SubscriptionName[];
      chainConnect?: boolean;
    }
  | { kind: "setLogLevel"; level: LogLevel }
  | { kind: "frame"; bytes: Uint8Array }
  | { kind: "disconnect"; requestId: number }
  | { kind: "callbackResponse"; requestId: number; ok: true; value: unknown }
  | { kind: "callbackResponse"; requestId: number; ok: false; error: string }
  | { kind: "subscriptionItem"; subId: number; value: unknown }
  | { kind: "chainConnectAck"; connId: number; ok: true }
  | { kind: "chainConnectAck"; connId: number; ok: false; error: string }
  | { kind: "chainResponse"; connId: number; json: string }
  | { kind: "dispose" };

export type WorkerToMain =
  | { kind: "loaded" }
  | { kind: "ready" }
  | { kind: "error"; error: string }
  | { kind: "frame"; bytes: Uint8Array }
  | { kind: "disconnectResponse"; requestId: number; ok: true }
  | { kind: "disconnectResponse"; requestId: number; ok: false; error: string }
  | {
      kind: "callbackRequest";
      requestId: number;
      name: CallbackName;
      args: CallbackArgs;
    }
  | {
      kind: "subscriptionStart";
      subId: number;
      name: SubscriptionName;
      payload: Uint8Array | null;
    }
  | { kind: "subscriptionStop"; subId: number }
  | { kind: "chainConnectStart"; connId: number; genesisHash: string }
  | { kind: "chainSend"; connId: number; request: string }
  | { kind: "chainClose"; connId: number };
