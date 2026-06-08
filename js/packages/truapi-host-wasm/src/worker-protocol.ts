// Wire format between the main thread (`createWebWorkerProvider`) and the
// Web Worker that hosts the truapi-server WASM core.
//
// Frames (`kind: 'frame'`) carry SCALE-encoded `ProtocolMessage` bytes
// untouched in either direction. Everything else is a control message
// for callback dispatch, subscription bookkeeping, or chain connections.

/**
 * Names of every request/response style host callback the wasm core can
 * invoke. Names match the camelCase property keys of `WasmRawCallbacks`.
 */
export type CallbackName =
  | "navigateTo"
  | "pushNotification"
  | "cancelNotification"
  | "devicePermission"
  | "remotePermission"
  | "featureSupported"
  | "localStorageRead"
  | "localStorageWrite"
  | "localStorageClear"
  | "presentPairing"
  | "readSession"
  | "writeSession"
  | "clearSession"
  | "confirmSignPayload"
  | "confirmSignRaw"
  | "confirmCreateTransaction"
  | "confirmAccountAlias"
  | "confirmResourceAllocation"
  | "confirmPreimageSubmit"
  | "submitPreimage";

export type OptionalCallbackName =
  | "cancelNotification"
  | "presentPairing"
  | "readSession"
  | "writeSession"
  | "clearSession"
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
      debug: boolean;
      runtimeConfig?: unknown;
      optionalCallbacks?: readonly OptionalCallbackName[];
      optionalSubscriptions?: readonly SubscriptionName[];
      chainConnect?: boolean;
    }
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
