// Wire format between the main thread (`createWebWorkerProvider`) and the
// Web Worker that hosts the truapi-server WASM core.
//
//   Main window / host JS
//   ┌─────────────────────────────────────────────────────────────────┐
//   │ createWebWorkerProvider                                         │
//   │ host callbacks: storage, DOM prompts, chain provider, logging   │
//   └───────────────┬─────────────────────────────────────────────────┘
//                   │ MainToWorker: init, frame, callbackResponse,
//                   │               subscriptionItem, chainResponse
//                   v
//   Dedicated Worker
//   ┌─────────────────────────────────────────────────────────────────┐
//   │ truapi-server WASM HostCore                                     │
//   │ generated raw-callback proxy                                    │
//   └───────────────┬─────────────────────────────────────────────────┘
//                   │ WorkerToMain: frame, callbackRequest,
//                   │               subscriptionStart, chainConnect
//                   v
//   Main window dispatches those requests to the actual host callbacks.
//
// Frames (`kind: 'frame'`) carry SCALE-encoded `ProtocolMessage` bytes
// untouched in either direction. Everything else is a control message
// for callback dispatch, subscription bookkeeping, or chain connections.
//
// Frame bytes cross the boundary by structured clone, deliberately not as
// transferables: the sender keeps using its buffer (the worker side posts
// views into WASM memory) and frames are small, so the copy is the simpler
// safe choice.

import type { LogLevel, PermissionAuthorizationStatus } from "./runtime.js";
import type {
  CallbackName,
  SubscriptionName,
} from "./generated/worker-callbacks.js";
/**
 * Generated callback-name unions used by the worker transport. They keep the
 * hand-written protocol aligned with the Rust platform callback catalog.
 */
export type {
  CallbackName,
  SubscriptionName,
} from "./generated/worker-callbacks.js";

/**
 * Positional arguments for a callback. The wasm core calls each callback
 * at a fixed arity; a uniform `unknown[]` keeps the wire protocol simple.
 */
export type CallbackArgs = readonly unknown[];

/**
 * Messages posted by the main window to the WASM worker. These either control
 * worker/core lifecycle, forward encoded TrUAPI frames into the core, or return
 * host callback/subscription/chain responses requested by the worker.
 */
export type MainToWorker =
  | {
      kind: "init";
      logLevel: LogLevel;
      runtimeConfig: unknown;
    }
  | { kind: "setLogLevel"; level: LogLevel }
  | { kind: "frame"; bytes: Uint8Array }
  | { kind: "disconnectSession"; requestId: number }
  | { kind: "cancelPairing" }
  | { kind: "notifySessionStoreChanged" }
  | {
      kind: "getPermissionAuthorizationStatus";
      requestId: number;
      request: Uint8Array;
    }
  | {
      kind: "getPermissionAuthorizationStatuses";
      requestId: number;
      requests: Uint8Array[];
    }
  | {
      kind: "setPermissionAuthorizationStatus";
      requestId: number;
      request: Uint8Array;
      status: PermissionAuthorizationStatus;
    }
  | { kind: "callbackResponse"; requestId: number; ok: true; value: unknown }
  | { kind: "callbackResponse"; requestId: number; ok: false; error: string }
  | { kind: "subscriptionItem"; subId: number; value: unknown }
  | { kind: "chainConnectAck"; connId: number; ok: true }
  | { kind: "chainConnectAck"; connId: number; ok: false; error: string }
  | { kind: "chainResponse"; connId: number; json: string }
  | { kind: "dispose" };

/**
 * Messages posted by the WASM worker back to the main window. These either
 * report worker lifecycle/errors, emit encoded TrUAPI frames from the core, or
 * request host callbacks, subscriptions, and chain-provider operations.
 */
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
      kind: "permissionAuthorizationStatusResponse";
      requestId: number;
      ok: true;
      status: PermissionAuthorizationStatus;
    }
  | {
      kind: "permissionAuthorizationStatusResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "permissionAuthorizationStatusesResponse";
      requestId: number;
      ok: true;
      statuses: PermissionAuthorizationStatus[];
    }
  | {
      kind: "permissionAuthorizationStatusesResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "setPermissionAuthorizationStatusResponse";
      requestId: number;
      ok: true;
    }
  | {
      kind: "setPermissionAuthorizationStatusResponse";
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
