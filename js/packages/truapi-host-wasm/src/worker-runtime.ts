/// <reference lib="webworker" />
// Worker entrypoint. Loads the web-targeted truapi-server WASM bundle and
// bridges every host callback over postMessage. The main thread keeps the
// state that needs DOM access (localStorage, prompts) while the CPU-heavy
// smoldot/dispatcher work runs here off the page main thread.

import type {
  CallbackName,
  MainToWorker,
  OptionalCallbackName,
  SubscriptionName,
  WorkerToMain,
} from "./worker-protocol.js";

interface WasmCore {
  receiveFromProduct(frame: Uint8Array): Promise<void>;
  disconnect(): Promise<void>;
  dispose(): void;
  free(): void;
}

interface WasmModuleShape {
  default: (input?: unknown) => Promise<unknown>;
  WasmTrUApiCore: new (callbacks: unknown, runtimeConfig?: unknown) => WasmCore;
  setDebugEnabled: (enabled: boolean) => void;
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

function errMsg(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return JSON.stringify(err);
}

let nextRequestId = 0;
const pendingCallbacks = new Map<
  number,
  (
    result: { ok: true; value: unknown } | { ok: false; error: string },
  ) => void
>();

let nextSubId = 0;
const subscriptionItemListeners = new Map<
  number,
  (value: unknown) => void
>();

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

type RawCallbackFn = (...args: never[]) => unknown;

const requiredRawCallbacks: Record<string, RawCallbackFn> = {
  navigateTo: (url: string) => callbackRequest("navigateTo", [url]),
  pushNotification: (payload: Uint8Array) =>
    callbackRequest("pushNotification", [payload]),
  devicePermission: (payload: Uint8Array) =>
    callbackRequest("devicePermission", [payload]) as Promise<boolean>,
  remotePermission: (payload: Uint8Array) =>
    callbackRequest("remotePermission", [payload]) as Promise<boolean>,
  featureSupported: (payload: Uint8Array) =>
    callbackRequest("featureSupported", [payload]) as Promise<boolean>,
  localStorageRead: (key: string) =>
    callbackRequest("localStorageRead", [key]) as Promise<
      Uint8Array | null | undefined
    >,
  localStorageWrite: (key: string, value: Uint8Array) =>
    callbackRequest("localStorageWrite", [key, value]),
  localStorageClear: (key: string) =>
    callbackRequest("localStorageClear", [key]),
};

const optionalRawCallbacks: Record<OptionalCallbackName, RawCallbackFn> = {
  cancelNotification: (id: number) =>
    callbackRequest("cancelNotification", [id]),
  presentPairing: (deeplink: string) =>
    callbackRequest("presentPairing", [deeplink]),
  readSession: () =>
    callbackRequest("readSession", []) as Promise<
      Uint8Array | null | undefined
    >,
  writeSession: (value: Uint8Array) =>
    callbackRequest("writeSession", [value]),
  clearSession: () => callbackRequest("clearSession", []),
  confirmSignPayload: (payload: Uint8Array) =>
    callbackRequest("confirmSignPayload", [payload]) as Promise<boolean>,
  confirmSignRaw: (payload: Uint8Array) =>
    callbackRequest("confirmSignRaw", [payload]) as Promise<boolean>,
  confirmCreateTransaction: (payload: Uint8Array) =>
    callbackRequest("confirmCreateTransaction", [payload]) as Promise<boolean>,
  confirmAccountAlias: (payload: Uint8Array) =>
    callbackRequest("confirmAccountAlias", [payload]) as Promise<boolean>,
  confirmResourceAllocation: (payload: Uint8Array) =>
    callbackRequest("confirmResourceAllocation", [payload]) as Promise<boolean>,
  confirmPreimageSubmit: (size: number) =>
    callbackRequest("confirmPreimageSubmit", [size]) as Promise<void>,
  submitPreimage: (value: Uint8Array) =>
    callbackRequest("submitPreimage", [value]) as Promise<Uint8Array>,
};

function buildRawCallbacks(msg: Extract<MainToWorker, { kind: "init" }>) {
  const callbacks: Record<string, unknown> = { ...requiredRawCallbacks };
  for (const name of msg.optionalCallbacks ?? []) {
    callbacks[name] = optionalRawCallbacks[name];
  }
  const optionalSubscriptions = new Set(msg.optionalSubscriptions ?? []);
  if (optionalSubscriptions.has("sessionStoreSubscribe")) {
    callbacks.subscribeSessionStore = (sendItem: () => void) =>
      startSubscription("sessionStoreSubscribe", null, sendItem);
  }
  if (optionalSubscriptions.has("themeSubscribe")) {
    callbacks.themeSubscribe = (
      sendItem: (theme: "Light" | "Dark" | 0 | 1 | Uint8Array) => void,
    ) => startSubscription("themeSubscribe", null, sendItem);
  }
  if (optionalSubscriptions.has("preimageLookupSubscribe")) {
    callbacks.preimageLookupSubscribe = (
      key: Uint8Array,
      sendItem: (value: Uint8Array | null | undefined) => void,
    ) => startSubscription("preimageLookupSubscribe", key, sendItem);
  }
  if (msg.chainConnect) {
    callbacks.chainConnect = chainConnect;
  }
  callbacks.emitFrame = (frame: Uint8Array): void => {
    postToMain({ kind: "frame", bytes: frame });
  };
  callbacks.dispose = (): void => {
    // Main thread terminates the worker, no separate cleanup needed here.
  };
  return callbacks;
}

let core: WasmCore | null = null;
let wasm: WasmModuleShape | null = null;

(async () => {
  try {
    wasm = await wasmModulePromise;
    await wasm.default();
    postToMain({ kind: "loaded" });
  } catch (err) {
    postToMain({ kind: "error", error: errMsg(err) });
  }
})();

ctx.addEventListener("message", (ev: MessageEvent<MainToWorker>) => {
  const msg = ev.data;
  switch (msg.kind) {
    case "init":
      if (!wasm) {
        postToMain({ kind: "error", error: "init received before WASM loaded" });
        break;
      }
      wasm?.setDebugEnabled(msg.debug);
      try {
        core = new wasm.WasmTrUApiCore(buildRawCallbacks(msg), msg.runtimeConfig);
        postToMain({ kind: "ready" });
      } catch (err) {
        postToMain({ kind: "error", error: `init: ${errMsg(err)}` });
      }
      break;
    case "frame":
      void handleFrame(msg.bytes);
      break;
    case "disconnect":
      void handleDisconnect(msg.requestId);
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
        postToMain({ kind: "error", error: `dispose: ${errMsg(err)}` });
      }
      core = null;
      break;
  }
});

async function handleDisconnect(requestId: number): Promise<void> {
  if (!core) {
    postToMain({
      kind: "disconnectResponse",
      requestId,
      ok: false,
      error: "disconnect received before core is ready",
    });
    return;
  }
  try {
    await core.disconnect();
    postToMain({ kind: "disconnectResponse", requestId, ok: true });
  } catch (err) {
    postToMain({
      kind: "disconnectResponse",
      requestId,
      ok: false,
      error: errMsg(err),
    });
  }
}

async function handleFrame(bytes: Uint8Array): Promise<void> {
  if (!core) {
    postToMain({ kind: "error", error: "frame received before core is ready" });
    return;
  }
  try {
    await core.receiveFromProduct(bytes);
  } catch (err) {
    postToMain({ kind: "error", error: `receiveFromProduct: ${errMsg(err)}` });
  }
}
