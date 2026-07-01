/// <reference lib="webworker" />
// Worker entrypoint. Loads the web-targeted truapi-server WASM bundle and
// bridges every host callback over postMessage. The main thread keeps the
// state that needs DOM access (localStorage, prompts) while the core dispatcher
// runs here off the page main thread.

import type {
  MainToWorker,
  SubscriptionName,
  WorkerToMain,
} from "./worker-protocol.js";
import {
  createWorkerRawCallbacks,
  type CallbackName,
} from "./generated/worker-callbacks.js";
import { errorMessage } from "./error.js";

type PermissionAuthorizationStatus =
  | "NotDetermined"
  | "Denied"
  | "Authorized";

interface WorkerHostCore {
  receiveFrame(frame: Uint8Array): Promise<void>;
  disconnectSession(): Promise<void>;
  cancelPairing(): void;
  notifySessionStoreChanged(): void;
  permissionAuthorizationStatus(request: Uint8Array): Promise<string>;
  permissionAuthorizationStatuses(requests: Uint8Array[]): Promise<string[]>;
  setPermissionAuthorizationStatus(
    request: Uint8Array,
    status: string,
  ): Promise<void>;
  dispose(): void;
  free(): void;
}

interface WasmModuleShape {
  default: (input?: unknown) => Promise<unknown>;
  WasmHostCore: new (
    callbacks: unknown,
    runtimeConfig: unknown,
  ) => WorkerHostCore;
  setLogLevel?: (level: string) => void;
}

// Resolved at runtime, the wasm-pack artifact lives outside `src/` so a
// static import would leak into the TS rootDir. The relative path is
// resolved against `dist/worker-runtime.js` once compiled. Indirected
// through a variable so TS skips the static module-existence check.
const WASM_WEB_PATH = "./wasm/web/truapi_server.js";
const wasmModulePromise = import(
  /* @vite-ignore */ WASM_WEB_PATH
) as Promise<WasmModuleShape>;

const ctx = self as unknown as DedicatedWorkerGlobalScope;

function postToMain(msg: WorkerToMain): void {
  ctx.postMessage(msg);
}

let nextRequestId = 0;
const pendingCallbacks = new Map<
  number,
  (result: { ok: true; value: unknown } | { ok: false; error: string }) => void
>();

let nextSubId = 0;
const subscriptionItemListeners = new Map<number, (value: unknown) => void>();

let nextConnId = 0;
type ChainConnectAck = { ok: true } | { ok: false; error: string };
const chainConnectAcks = new Map<number, (ack: ChainConnectAck) => void>();
const chainResponseListeners = new Map<number, (json: string) => void>();

function callbackRequest(
  name: CallbackName,
  args: readonly unknown[],
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const requestId = ++nextRequestId;
    pendingCallbacks.set(requestId, (r) => {
      if (r.ok) resolve(r.value);
      else reject(new Error(r.error));
    });
    postToMain({ kind: "callbackRequest", requestId, name, args });
  });
}

function startSubscription<T>(
  name: SubscriptionName,
  payload: Uint8Array | null,
  sendItem: (value: T) => void,
): () => void {
  const subId = ++nextSubId;
  subscriptionItemListeners.set(subId, sendItem as (value: unknown) => void);
  postToMain({ kind: "subscriptionStart", subId, name, payload });
  return () => {
    subscriptionItemListeners.delete(subId);
    postToMain({ kind: "subscriptionStop", subId });
  };
}

interface WorkerChainConnection {
  send(request: string): void;
  close(): void;
}

/**
 * Worker-side half of the host chain-connect bridge.
 *
 * The Rust core runs in this worker but owns no socket. When it needs chain
 * access (chainHead v1 for People-chain identity / statement-store SSO) it
 * calls this; the actual transport lives on the host main thread and is reached
 * over postMessage. The data crossing here is JSON-RPC strings, not SCALE: only
 * the product<->core wire is SCALE.
 *
 *   per-tab / sandboxed          core-owned (this Web Worker)       host-owned (main thread)
 *   +-------------------+  SCALE  +--------------------------+      +--------------------------------+
 *   | Product (iframe)  |<------->| truapi-server WASM core  |      | host.connect() (ChainProvider) |
 *   | speaks TrUAPI     |  frames | chainHead v1, SSO,       |      | host-owned JSON-RPC transport  |
 *   | never sees chains |         | People-chain identity    |      | remote RPC, native client, ... |
 *   +-------------------+         +--------------------------+      +--------------------------------+
 *                                      |   ^  JSON-RPC strings (not SCALE)        ^   |
 *                       chainConnect() |   | onResponse(json)           connect   |   | responses()
 *                         (this fn)    v   |                                      |   v
 *                 worker-runtime.ts  <======== postMessage ========>  create-worker-host-runtime.ts
 *                 chainConnectStart / chainSend / chainClose   -->   handleChainConnect* -> host.connect()
 *                 chainConnectAck   / chainResponse            <--   (pumped from connection.responses())
 *
 * Allocates a `connId`, posts `chainConnectStart`, and resolves a
 * `{ send, close }` handle once the main thread acks. `send` posts `chainSend`,
 * `close` posts `chainClose`, and every `chainResponse` for this `connId` is
 * delivered to `onResponse`.
 */
function chainConnect(
  genesisHash: string,
  onResponse: (json: string) => void,
): Promise<WorkerChainConnection | null> {
  const connId = ++nextConnId;
  return new Promise((resolve, reject) => {
    chainConnectAcks.set(connId, (ack) => {
      if (!ack.ok) {
        chainResponseListeners.delete(connId);
        reject(new Error(ack.error));
        return;
      }
      resolve({
        send(request: string) {
          postToMain({ kind: "chainSend", connId, request });
        },
        close() {
          chainResponseListeners.delete(connId);
          postToMain({ kind: "chainClose", connId });
        },
      });
    });
    chainResponseListeners.set(connId, onResponse);
    postToMain({ kind: "chainConnectStart", connId, genesisHash });
  });
}

/**
 * Build the callback object passed to the WASM core. Most entries are
 * generated proxy functions that bounce from the worker to the main window;
 * `emitFrame` is filled here because it is the core-to-provider data path.
 */
function buildRawCallbacks() {
  const callbacks = createWorkerRawCallbacks({
    callbackRequest,
    startSubscription,
    chainConnect,
  });
  callbacks.emitFrame = (frame: Uint8Array): void => {
    postToMain({ kind: "frame", bytes: frame });
  };
  callbacks.dispose = (): void => {
    // Main thread terminates the worker, no separate cleanup needed here.
  };
  return callbacks;
}

let core: WorkerHostCore | null = null;
let wasm: WasmModuleShape | null = null;

(async () => {
  try {
    wasm = await wasmModulePromise;
    await wasm.default();
    postToMain({ kind: "loaded" });
  } catch (err) {
    postToMain({ kind: "fatalError", error: errorMessage(err) });
  }
})();

ctx.addEventListener("message", (ev: MessageEvent<MainToWorker>) => {
  const msg = ev.data;
  switch (msg.kind) {
    case "init":
      if (!wasm) {
        postToMain({
          kind: "fatalError",
          error: "init received before WASM loaded",
        });
        break;
      }
      if (core) {
        postToMain({
          kind: "fatalError",
          error: "init: core already initialized",
        });
        break;
      }
      wasm.setLogLevel?.(msg.logLevel);
      try {
        core = new wasm.WasmHostCore(buildRawCallbacks(), msg.runtimeConfig);
        postToMain({ kind: "ready" });
      } catch (err) {
        postToMain({ kind: "fatalError", error: `init: ${errorMessage(err)}` });
      }
      break;
    case "setLogLevel":
      wasm?.setLogLevel?.(msg.level);
      break;
    case "frame":
      void handleFrame(msg.bytes);
      break;
    case "disconnectSession":
      void handleDisconnectSession(msg.requestId);
      break;
    case "cancelPairing":
      core?.cancelPairing();
      break;
    case "notifySessionStoreChanged":
      core?.notifySessionStoreChanged();
      break;
    case "getPermissionAuthorizationStatus":
      void handleGetPermissionAuthorizationStatus(msg.requestId, msg.request);
      break;
    case "getPermissionAuthorizationStatuses":
      void handleGetPermissionAuthorizationStatuses(
        msg.requestId,
        msg.requests,
      );
      break;
    case "setPermissionAuthorizationStatus":
      void handleSetPermissionAuthorizationStatus(
        msg.requestId,
        msg.request,
        msg.status,
      );
      break;
    case "callbackResponse": {
      const cb = pendingCallbacks.get(msg.requestId);
      if (cb) {
        pendingCallbacks.delete(msg.requestId);
        cb(
          msg.ok
            ? { ok: true, value: msg.value }
            : { ok: false, error: msg.error },
        );
      }
      break;
    }
    case "subscriptionItem": {
      const listener = subscriptionItemListeners.get(msg.subId);
      if (listener) listener(msg.value);
      break;
    }
    case "chainConnectAck": {
      const cb = chainConnectAcks.get(msg.connId);
      if (cb) {
        chainConnectAcks.delete(msg.connId);
        cb(msg.ok ? { ok: true } : { ok: false, error: msg.error });
      }
      break;
    }
    case "chainResponse": {
      const listener = chainResponseListeners.get(msg.connId);
      if (listener) listener(msg.json);
      break;
    }
    case "dispose":
      try {
        core?.dispose();
        core?.free();
      } catch (err) {
        postToMain({ kind: "disposeError", error: errorMessage(err) });
      }
      core = null;
      break;
    default: {
      const { kind } = msg as { kind?: unknown };
      console.warn(
        `[truapi worker-runtime] unknown message kind: ${String(kind)}`,
      );
    }
  }
});

async function handleDisconnectSession(requestId: number): Promise<void> {
  if (!core) {
    postToMain({
      kind: "disconnectSessionResponse",
      requestId,
      ok: false,
      error: "disconnectSession received before core is ready",
    });
    return;
  }
  try {
    await core.disconnectSession();
    postToMain({ kind: "disconnectSessionResponse", requestId, ok: true });
  } catch (err) {
    postToMain({
      kind: "disconnectSessionResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleGetPermissionAuthorizationStatus(
  requestId: number,
  request: Uint8Array,
): Promise<void> {
  if (!core) {
    postToMain({
      kind: "permissionAuthorizationStatusResponse",
      requestId,
      ok: false,
      error: "permissionAuthorizationStatus received before core is ready",
    });
    return;
  }
  try {
    const status = await core.permissionAuthorizationStatus(request);
    postToMain({
      kind: "permissionAuthorizationStatusResponse",
      requestId,
      ok: true,
      status: status as PermissionAuthorizationStatus,
    });
  } catch (err) {
    postToMain({
      kind: "permissionAuthorizationStatusResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleGetPermissionAuthorizationStatuses(
  requestId: number,
  requests: Uint8Array[],
): Promise<void> {
  if (!core) {
    postToMain({
      kind: "permissionAuthorizationStatusesResponse",
      requestId,
      ok: false,
      error: "permissionAuthorizationStatuses received before core is ready",
    });
    return;
  }
  try {
    const statuses = await core.permissionAuthorizationStatuses(requests);
    postToMain({
      kind: "permissionAuthorizationStatusesResponse",
      requestId,
      ok: true,
      statuses: statuses as PermissionAuthorizationStatus[],
    });
  } catch (err) {
    postToMain({
      kind: "permissionAuthorizationStatusesResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleSetPermissionAuthorizationStatus(
  requestId: number,
  request: Uint8Array,
  status: PermissionAuthorizationStatus,
): Promise<void> {
  if (!core) {
    postToMain({
      kind: "setPermissionAuthorizationStatusResponse",
      requestId,
      ok: false,
      error: "setPermissionAuthorizationStatus received before core is ready",
    });
    return;
  }
  try {
    await core.setPermissionAuthorizationStatus(request, status);
    postToMain({
      kind: "setPermissionAuthorizationStatusResponse",
      requestId,
      ok: true,
    });
  } catch (err) {
    postToMain({
      kind: "setPermissionAuthorizationStatusResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleFrame(bytes: Uint8Array): Promise<void> {
  if (!core) {
    postToMain({
      kind: "frameError",
      error: "frame received before core is ready",
    });
    return;
  }
  try {
    await core.receiveFrame(bytes);
  } catch (err) {
    postToMain({
      kind: "frameError",
      error: errorMessage(err),
    });
  }
}
