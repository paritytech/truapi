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

export type CallbackName =
  | "navigateTo"
  | "pushNotification"
  | "cancelNotification"
  | "devicePermission"
  | "remotePermission"
  | "featureSupported"
  | "read"
  | "write"
  | "clear"
  | "authStateChanged"
  | "readStoredSession"
  | "writeStoredSession"
  | "clearStoredSession"
  | "confirmSignPayload"
  | "confirmSignRaw"
  | "confirmCreateTransaction"
  | "confirmAccountAlias"
  | "confirmResourceAllocation"
  | "confirmPreimageSubmit"
  | "submitPreimage";

export type OptionalCallbackName =
  | "cancelNotification"
  | "authStateChanged"
  | "readStoredSession"
  | "writeStoredSession"
  | "clearStoredSession"
  | "confirmSignPayload"
  | "confirmSignRaw"
  | "confirmCreateTransaction"
  | "confirmAccountAlias"
  | "confirmResourceAllocation"
  | "confirmPreimageSubmit"
  | "submitPreimage";

/**
 * Names of every subscription host callback. Each has the shape
 * `(payload?, sendItem) => dispose | void`.
 */
export type SubscriptionName =
  | "subscribeStoredSession"
  | "lookupPreimage"
  | "subscribeTheme";

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
  | { kind: "disconnectSession"; requestId: number }
  | { kind: "cancelPairing" }
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
  | { kind: "fatalError"; error: string }
  | { kind: "frameError"; error: string }
  | { kind: "disposeError"; error: string }
  | { kind: "frame"; bytes: Uint8Array }
  | { kind: "disconnectSessionResponse"; requestId: number; ok: true }
  | {
      kind: "disconnectSessionResponse";
      requestId: number;
      ok: false;
      error: string;
    }
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
