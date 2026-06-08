import type {
  CallbackName,
  ChainConnection,
  MainToWorker,
  OptionalCallbackName,
  SubscriptionName,
  TrUApiHostWasmProvider,
  WasmRuntimeConfig,
  WasmRawCallbacks,
  WorkerToMain,
} from "../index.js";

interface WorkerProviderState {
  worker: Worker;
  rawCallbacks: WasmRawCallbacks;
  listeners: Set<(message: Uint8Array) => void>;
  closeListeners: Set<(error: Error) => void>;
  subscriptionDisposers: Map<number, () => void>;
  chainConnections: Map<number, ChainConnection>;
  pendingDisconnects: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  disposed: boolean;
}

let nextDisconnectRequestId = 0;

const OPTIONAL_CALLBACK_NAMES: readonly OptionalCallbackName[] = [
  "cancelNotification",
  "presentPairing",
  "readSession",
  "writeSession",
  "clearSession",
  "confirmSignPayload",
  "confirmSignRaw",
  "confirmCreateTransaction",
  "confirmAccountAlias",
  "confirmResourceAllocation",
  "confirmPreimageSubmit",
  "submitPreimage",
];

const OPTIONAL_SUBSCRIPTION_NAMES: readonly {
  readonly callback: keyof Omit<WasmRawCallbacks, "emitFrame">;
  readonly protocol: SubscriptionName;
}[] = [
  { callback: "subscribeSessionStore", protocol: "sessionStoreSubscribe" },
  { callback: "themeSubscribe", protocol: "themeSubscribe" },
  { callback: "preimageLookupSubscribe", protocol: "preimageLookupSubscribe" },
];

function optionalCallbacks(
  callbacks: Omit<WasmRawCallbacks, "emitFrame">,
): OptionalCallbackName[] {
  return OPTIONAL_CALLBACK_NAMES.filter(
    (name) => typeof callbacks[name] === "function",
  );
}

function optionalSubscriptions(
  callbacks: Omit<WasmRawCallbacks, "emitFrame">,
): SubscriptionName[] {
  return OPTIONAL_SUBSCRIPTION_NAMES.filter(
    ({ callback }) => typeof callbacks[callback] === "function",
  ).map(({ protocol }) => protocol);
}

function errMsg(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return JSON.stringify(err);
}

function handleCallbackRequest(
  state: WorkerProviderState,
  msg: {
    requestId: number;
    name: CallbackName;
    args: readonly unknown[];
  },
): void {
  const fn = (
    state.rawCallbacks as unknown as Record<
      string,
      (...args: readonly unknown[]) => unknown
    >
  )[msg.name];
  if (!fn) {
    const reply: MainToWorker = {
      kind: "callbackResponse",
      requestId: msg.requestId,
      ok: false,
      error: `unknown callback: ${msg.name}`,
    };
    state.worker.postMessage(reply);
    return;
  }
  Promise.resolve()
    .then(() => fn(...msg.args))
    .then(
      (value) => {
        const reply: MainToWorker = {
          kind: "callbackResponse",
          requestId: msg.requestId,
          ok: true,
          value,
        };
        state.worker.postMessage(reply);
      },
      (err) => {
        const reply: MainToWorker = {
          kind: "callbackResponse",
          requestId: msg.requestId,
          ok: false,
          error: errMsg(err),
        };
        state.worker.postMessage(reply);
      },
    );
}

function handleSubscriptionStart(
  state: WorkerProviderState,
  msg: {
    subId: number;
    name: SubscriptionName;
    payload: Uint8Array | null;
  },
): void {
  const sendItem = (value: unknown): void => {
    if (state.disposed) return;
    const post: MainToWorker = {
      kind: "subscriptionItem",
      subId: msg.subId,
      value,
    };
    state.worker.postMessage(post);
  };
  let dispose: unknown;
  try {
    if (msg.name === "sessionStoreSubscribe") {
      dispose = state.rawCallbacks.subscribeSessionStore?.(
        sendItem as () => void,
      );
    } else if (msg.name === "themeSubscribe") {
      dispose = state.rawCallbacks.themeSubscribe?.(
        sendItem as (theme: "Light" | "Dark" | 0 | 1 | Uint8Array) => void,
      );
    } else if (msg.payload !== null) {
      dispose = state.rawCallbacks.preimageLookupSubscribe(
        msg.payload,
        sendItem as (value: Uint8Array | null | undefined) => void,
      );
    } else {
      console.warn(
        `[truapi worker] ${msg.name} requires payload, none received`,
      );
      return;
    }
  } catch (err) {
    console.error(`[truapi worker] ${msg.name} threw on start:`, err);
    return;
  }
  if (typeof dispose === "function") {
    state.subscriptionDisposers.set(msg.subId, dispose as () => void);
  }
}

function handleSubscriptionStop(
  state: WorkerProviderState,
  msg: { subId: number },
): void {
  const dispose = state.subscriptionDisposers.get(msg.subId);
  if (!dispose) return;
  state.subscriptionDisposers.delete(msg.subId);
  try {
    dispose();
  } catch (err) {
    console.warn("[truapi worker] subscription dispose threw:", err);
  }
}

async function handleChainConnectStart(
  state: WorkerProviderState,
  msg: { connId: number; genesisHash: string },
): Promise<void> {
  const chainConnect = state.rawCallbacks.chainConnect;
  if (!chainConnect) {
    const reply: MainToWorker = {
      kind: "chainConnectAck",
      connId: msg.connId,
      ok: false,
      error: "host did not supply chainConnect",
    };
    state.worker.postMessage(reply);
    return;
  }
  const onResponse = (json: string): void => {
    if (state.disposed) return;
    const post: MainToWorker = {
      kind: "chainResponse",
      connId: msg.connId,
      json,
    };
    state.worker.postMessage(post);
  };
  try {
    const conn = await chainConnect(msg.genesisHash, onResponse);
    if (!conn) {
      const reply: MainToWorker = {
        kind: "chainConnectAck",
        connId: msg.connId,
        ok: false,
        error: `chainConnect returned null for genesisHash ${msg.genesisHash}`,
      };
      state.worker.postMessage(reply);
      return;
    }
    state.chainConnections.set(msg.connId, conn);
    const reply: MainToWorker = {
      kind: "chainConnectAck",
      connId: msg.connId,
      ok: true,
    };
    state.worker.postMessage(reply);
  } catch (err) {
    const reply: MainToWorker = {
      kind: "chainConnectAck",
      connId: msg.connId,
      ok: false,
      error: errMsg(err),
    };
    state.worker.postMessage(reply);
  }
}

function handleChainSend(
  state: WorkerProviderState,
  msg: { connId: number; request: string },
): void {
  const conn = state.chainConnections.get(msg.connId);
  if (!conn) return;
  try {
    conn.send(msg.request);
  } catch (err) {
    console.warn("[truapi worker] chain send threw:", err);
  }
}

function handleChainClose(
  state: WorkerProviderState,
  msg: { connId: number },
): void {
  const conn = state.chainConnections.get(msg.connId);
  if (!conn) return;
  state.chainConnections.delete(msg.connId);
  try {
    conn.close();
  } catch (err) {
    console.warn("[truapi worker] chain close threw:", err);
  }
}

function handleDisconnectResponse(
  state: WorkerProviderState,
  msg:
    | { requestId: number; ok: true }
    | { requestId: number; ok: false; error: string },
): void {
  const pending = state.pendingDisconnects.get(msg.requestId);
  if (!pending) return;
  state.pendingDisconnects.delete(msg.requestId);
  if (msg.ok) {
    pending.resolve();
  } else {
    pending.reject(new Error(msg.error));
  }
}

function rejectPendingDisconnects(state: WorkerProviderState, error: Error): void {
  for (const pending of state.pendingDisconnects.values()) {
    pending.reject(error);
  }
  state.pendingDisconnects.clear();
}

export interface CreateWebWorkerProviderOptions {
  /** Toggle the wasm core's debug logging. Default: `false`. */
  debug?: boolean;
  /** Static product/pairing config passed to the Rust core. */
  runtimeConfig?: WasmRuntimeConfig;
}

/**
 * Spawn the truapi-server WASM in `worker` and bridge it into a
 * `Provider`. The provider can be handed to `createHostServer` from
 * `@parity/truapi-host`.
 *
 * The caller is responsible for instantiating the Worker, Vite users
 * typically import the worker entry-point with `?worker`:
 *
 * ```ts
 * import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
 * const worker = new HostWorker();
 * const provider = await createWebWorkerProvider(worker, callbacks);
 * ```
 *
 * Resolves once the worker reports `ready` and rejects if the WASM
 * fails to load.
 */
export function createWebWorkerProvider(
  worker: Worker,
  callbacks: Omit<WasmRawCallbacks, "emitFrame">,
  options: CreateWebWorkerProviderOptions = {},
): Promise<TrUApiHostWasmProvider> {
  return new Promise((resolve, reject) => {
    const state: WorkerProviderState = {
      worker,
      // `emitFrame` is satisfied by the worker side; main thread never
      // calls it. Fill in a no-op so the typed callback set is complete.
      rawCallbacks: {
        ...(callbacks as WasmRawCallbacks),
        emitFrame: () => {},
      },
      listeners: new Set(),
      closeListeners: new Set(),
      subscriptionDisposers: new Map(),
      chainConnections: new Map(),
      pendingDisconnects: new Map(),
      disposed: false,
    };

    const onMessage = (ev: MessageEvent<WorkerToMain>): void => {
      const msg = ev.data;
      switch (msg.kind) {
        case "loaded":
          break;
        case "ready":
          break;
        case "error":
          console.error("[truapi worker]", msg.error);
          for (const listener of [...state.closeListeners])
            listener(new Error(msg.error));
          break;
        case "frame":
          for (const listener of [...state.listeners]) listener(msg.bytes);
          break;
        case "disconnectResponse":
          handleDisconnectResponse(state, msg);
          break;
        case "callbackRequest":
          handleCallbackRequest(state, msg);
          break;
        case "subscriptionStart":
          handleSubscriptionStart(state, msg);
          break;
        case "subscriptionStop":
          handleSubscriptionStop(state, msg);
          break;
        case "chainConnectStart":
          void handleChainConnectStart(state, msg);
          break;
        case "chainSend":
          handleChainSend(state, msg);
          break;
        case "chainClose":
          handleChainClose(state, msg);
          break;
      }
    };

    const notifyFault = (error: Error): void => {
      if (state.disposed) return;
      state.disposed = true;
      rejectPendingDisconnects(state, error);
      for (const listener of [...state.closeListeners]) listener(error);
      state.listeners.clear();
      state.closeListeners.clear();
    };

    const onError = (e: ErrorEvent): void => {
      cleanupInit();
      worker.terminate();
      reject(new Error(`worker init failed: ${e.message}`));
    };

    const onRuntimeError = (e: ErrorEvent): void => {
      console.error("[truapi worker]", e.message);
      notifyFault(new Error(`worker error: ${e.message}`));
    };

    const onMessageError = (): void => {
      notifyFault(new Error("worker message could not be deserialized"));
    };

    const onInitMessage = (ev: MessageEvent<WorkerToMain>): void => {
      const msg = ev.data;
      if (msg.kind === "loaded") {
        const init: MainToWorker = {
          kind: "init",
          debug: options.debug ?? false,
          runtimeConfig: options.runtimeConfig,
          optionalCallbacks: optionalCallbacks(callbacks),
          optionalSubscriptions: optionalSubscriptions(callbacks),
          chainConnect: typeof callbacks.chainConnect === "function",
        };
        worker.postMessage(init);
      } else if (msg.kind === "ready") {
        cleanupInit();
        worker.addEventListener("message", onMessage);
        // Surface a post-init worker fault (uncaught throw, OOM, killed
        // worker) to close listeners for the provider's lifetime.
        worker.addEventListener("error", onRuntimeError);
        worker.addEventListener("messageerror", onMessageError);
        resolve(buildProvider(state));
      } else if (msg.kind === "error") {
        cleanupInit();
        worker.terminate();
        reject(new Error(`worker init reported error: ${msg.error}`));
      }
    };

    const cleanupInit = (): void => {
      worker.removeEventListener("error", onError);
      worker.removeEventListener("message", onInitMessage);
    };

    worker.addEventListener("error", onError);
    worker.addEventListener("message", onInitMessage);
  });
}

function buildProvider(state: WorkerProviderState): TrUApiHostWasmProvider {
  return {
    postMessage(bytes: Uint8Array): void {
      if (state.disposed) return;
      const post: MainToWorker = { kind: "frame", bytes };
      state.worker.postMessage(post);
    },
    subscribe(callback) {
      state.listeners.add(callback);
      return () => {
        state.listeners.delete(callback);
      };
    },
    subscribeClose(callback) {
      state.closeListeners.add(callback);
      return () => {
        state.closeListeners.delete(callback);
      };
    },
    disconnect(): Promise<void> {
      if (state.disposed) return Promise.resolve();
      return new Promise((resolve, reject) => {
        const requestId = ++nextDisconnectRequestId;
        state.pendingDisconnects.set(requestId, { resolve, reject });
        try {
          const post: MainToWorker = { kind: "disconnect", requestId };
          state.worker.postMessage(post);
        } catch (err) {
          state.pendingDisconnects.delete(requestId);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    },
    dispose() {
      if (state.disposed) return;
      state.disposed = true;
      rejectPendingDisconnects(state, new Error("provider disposed"));
      for (const fn of state.subscriptionDisposers.values()) {
        try {
          fn();
        } catch {
          // ignore during teardown
        }
      }
      state.subscriptionDisposers.clear();
      for (const conn of state.chainConnections.values()) {
        try {
          conn.close();
        } catch {
          // ignore during teardown
        }
      }
      state.chainConnections.clear();
      try {
        const post: MainToWorker = { kind: "dispose" };
        state.worker.postMessage(post);
      } catch {
        // ignore if worker already gone
      }
      state.worker.terminate();
      state.listeners.clear();
      state.closeListeners.clear();
    },
  };
}
