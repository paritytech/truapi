/// <reference lib="webworker" />
// Worker entrypoint. Loads the web-targeted truapi-server WASM bundle and
// bridges every host callback over postMessage. The main thread keeps the
// state that needs DOM access (localStorage, prompts) while the CPU-heavy
// smoldot/dispatcher work runs here off the page main thread.

import type {
  MainToWorker,
  SubscriptionName,
  WorkerToMain,
} from "./worker-protocol.js";
import {
  createWorkerRawCallbacks,
  type CallbackName,
} from "./generated/worker-callbacks.js";

interface WorkerHostCore {
  receiveFrame(frame: Uint8Array): Promise<void>;
  disconnectSession(): Promise<void>;
  cancelPairing(): void;
  notifySessionStoreChanged(): void;
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

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return JSON.stringify(err) ?? String(err);
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

function buildRawCallbacks(msg: Extract<MainToWorker, { kind: "init" }>) {
  const callbacks = createWorkerRawCallbacks({
    callbackRequest,
    startSubscription,
    ...(msg.chainConnect ? { chainConnect } : {}),
    optionalCallbacks: msg.optionalCallbacks,
    optionalSubscriptions: msg.optionalSubscriptions,
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
        core = new wasm.WasmHostCore(buildRawCallbacks(msg), msg.runtimeConfig);
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
