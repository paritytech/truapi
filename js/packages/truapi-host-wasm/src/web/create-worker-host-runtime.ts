import type {
  ChainConnection,
  HostCallbacks,
  LogLevel,
  PermissionAuthorizationRequest,
  PermissionAuthorizationStatus,
  TrUApiHostCoreProvider,
  HostCoreRuntimeConfig,
} from "../index.js";
import { PermissionAuthorizationRequest as PermissionAuthorizationRequestCodec } from "../generated/host-callbacks.js";
import { createWasmRawCallbacks } from "../generated/host-callbacks-adapter.js";
import type { RawCallbacks } from "../generated/host-callbacks-adapter.js";
import type {
  CallbackName,
  MainToWorker,
  SubscriptionName,
  WorkerToMain,
} from "../worker-protocol.js";
import { bytesToHex } from "@parity/truapi/scale";
import {
  implementedOptionalCallbacks,
  implementedOptionalSubscriptions,
  startRawSubscription,
} from "../generated/worker-callbacks.js";

interface WorkerProviderState {
  worker: Worker;
  rawCallbacks: RawCallbacks;
  listeners: Set<(message: Uint8Array) => void>;
  closeListeners: Set<(error: Error) => void>;
  subscriptionDisposers: Map<number, () => void>;
  chainConnections: Map<number, ChainConnection>;
  pendingDisconnects: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  pendingPermissionAuthorizationStatuses: Map<
    number,
    {
      resolve: (status: PermissionAuthorizationStatus) => void;
      reject: (error: Error) => void;
    }
  >;
  pendingSetPermissionAuthorizationStatuses: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  closedError: Error | null;
  logLevel: LogLevel;
  disposed: boolean;
}

function debugLoggingEnabled(state: WorkerProviderState): boolean {
  return state.logLevel === "debug" || state.logLevel === "trace";
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return JSON.stringify(err) ?? String(err);
}

let nextDisconnectRequestId = 0;
let nextPermissionAuthorizationRequestId = 0;

function encodePermissionAuthorizationRequest(
  request: PermissionAuthorizationRequest,
): Uint8Array {
  return PermissionAuthorizationRequestCodec.enc(request);
}

/** localStorage key the dev log level is persisted under, so it survives reloads. */
const DEV_LOG_LEVEL_KEY = "truapi:logLevel";

/** Read the persisted dev log level. Returns null when unset. */
function readPersistedLogLevel(): LogLevel | null {
  return localStorage.getItem(DEV_LOG_LEVEL_KEY);
}

/** Persist the dev log level so it re-applies on the next reload. */
function persistLogLevel(level: LogLevel): void {
  localStorage.setItem(DEV_LOG_LEVEL_KEY, level);
}

let devLogLevelOverride: LogLevel | null = readPersistedLogLevel();
const devGlobalProviders = new Set<TrUApiHostCoreProvider>();
interface TrUApiDevConsole {
  setLogLevel(level: LogLevel): void;
  getLogLevel(): LogLevel | null;
}

function handleCallbackRequest(
  state: WorkerProviderState,
  msg: {
    requestId: number;
    name: CallbackName;
    args: readonly unknown[];
  },
): void {
  // Own-property guard: `msg.name` is worker-supplied, never walk the
  // prototype chain with it.
  const fn = Object.hasOwn(state.rawCallbacks, msg.name)
    ? (
        state.rawCallbacks as unknown as Record<
          string,
          (...args: readonly unknown[]) => unknown
        >
      )[msg.name]
    : undefined;
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
          error: errorMessage(err),
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
  const sendItem = (value?: unknown): void => {
    if (state.disposed) return;
    const post: MainToWorker = {
      kind: "subscriptionItem",
      subId: msg.subId,
      value,
    };
    state.worker.postMessage(post);
  };
  let dispose: (() => void) | void = undefined;
  try {
    dispose = startRawSubscription(
      state.rawCallbacks,
      msg.name,
      msg.payload,
      sendItem,
    );
  } catch (err) {
    console.error(`[truapi worker] ${msg.name} threw on start:`, err);
    return;
  }
  if (typeof dispose === "function") {
    state.subscriptionDisposers.set(msg.subId, dispose);
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
      error: errorMessage(err),
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
    if (debugLoggingEnabled(state)) {
      console.debug("[truapi worker] chainSend", msg.connId, msg.request);
    }
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

function handlePermissionAuthorizationStatusResponse(
  state: WorkerProviderState,
  msg:
    | {
        requestId: number;
        ok: true;
        status: PermissionAuthorizationStatus;
      }
    | { requestId: number; ok: false; error: string },
): void {
  const pending = state.pendingPermissionAuthorizationStatuses.get(
    msg.requestId,
  );
  if (!pending) return;
  state.pendingPermissionAuthorizationStatuses.delete(msg.requestId);
  if (msg.ok) {
    pending.resolve(msg.status);
  } else {
    pending.reject(new Error(msg.error));
  }
}

function handleSetPermissionAuthorizationStatusResponse(
  state: WorkerProviderState,
  msg:
    | { requestId: number; ok: true }
    | { requestId: number; ok: false; error: string },
): void {
  const pending = state.pendingSetPermissionAuthorizationStatuses.get(
    msg.requestId,
  );
  if (!pending) return;
  state.pendingSetPermissionAuthorizationStatuses.delete(msg.requestId);
  if (msg.ok) {
    pending.resolve();
  } else {
    pending.reject(new Error(msg.error));
  }
}

function rejectPendingDisconnects(
  state: WorkerProviderState,
  error: Error,
): void {
  for (const pending of state.pendingDisconnects.values()) {
    pending.reject(error);
  }
  state.pendingDisconnects.clear();
}

function rejectPendingPermissionAuthorizationRequests(
  state: WorkerProviderState,
  error: Error,
): void {
  for (const pending of state.pendingPermissionAuthorizationStatuses.values()) {
    pending.reject(error);
  }
  state.pendingPermissionAuthorizationStatuses.clear();
  for (const pending of state.pendingSetPermissionAuthorizationStatuses.values()) {
    pending.reject(error);
  }
  state.pendingSetPermissionAuthorizationStatuses.clear();
}

/**
 * Shared terminal teardown for both `dispose()` and worker faults: rejects
 * pending disconnects, runs subscription disposers, closes chain connections,
 * and terminates the worker. A fault additionally notifies close listeners.
 */
function teardown(
  state: WorkerProviderState,
  error: Error,
  fault: boolean,
): void {
  if (state.disposed) return;
  state.disposed = true;
  state.closedError = error;
  rejectPendingDisconnects(state, error);
  rejectPendingPermissionAuthorizationRequests(state, error);
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
  if (fault) {
    state.worker.terminate();
  } else {
    try {
      const post: MainToWorker = { kind: "dispose" };
      state.worker.postMessage(post);
    } catch {
      // ignore if worker already gone
    }
    // Give the worker a tick to free the core before terminating.
    setTimeout(() => state.worker.terminate(), 0);
  }
  for (const listener of [...state.closeListeners]) listener(error);
  state.listeners.clear();
  state.closeListeners.clear();
}

export interface CreateWebWorkerProviderOptions {
  /** Wasm core log level. Default: `"off"`. */
  logLevel?: LogLevel;
  /** Static product/pairing config passed to the Rust core. */
  runtimeConfig: HostCoreRuntimeConfig;
  /**
   * Milliseconds to wait for the worker to report `ready` before rejecting
   * and terminating it. Default: 30000.
   */
  initTimeoutMs?: number;
}

/**
 * Spawn the truapi-server WASM in `worker` and bridge it into a
 * `WireProvider`.
 *
 * The caller is responsible for instantiating the Worker, Vite users
 * typically import the worker entry-point with `?worker`:
 *
 * ```ts
 * import HostWorker from "@parity/truapi-host-wasm/worker-runtime?worker";
 * const worker = new HostWorker();
 * const provider = await createWebWorkerProvider(worker, callbacks, {
 *   runtimeConfig,
 * });
 * ```
 *
 * Resolves once the worker reports `ready` and rejects if the WASM
 * fails to load.
 */
export function createWebWorkerProvider(
  worker: Worker,
  host: Partial<HostCallbacks>,
  options: CreateWebWorkerProviderOptions,
): Promise<TrUApiHostCoreProvider> {
  const callbacks = createWasmRawCallbacks(host);

  return new Promise((resolve, reject) => {
    const state: WorkerProviderState = {
      worker,
      rawCallbacks: callbacks,
      listeners: new Set(),
      closeListeners: new Set(),
      subscriptionDisposers: new Map(),
      chainConnections: new Map(),
      pendingDisconnects: new Map(),
      pendingPermissionAuthorizationStatuses: new Map(),
      pendingSetPermissionAuthorizationStatuses: new Map(),
      closedError: null,
      logLevel: devLogLevelOverride ?? options.logLevel ?? "off",
      disposed: false,
    };

    const onMessage = (ev: MessageEvent<WorkerToMain>): void => {
      const msg = ev.data;
      switch (msg.kind) {
        case "loaded":
          break;
        case "ready":
          break;
        case "fatalError":
          console.error("[truapi worker]", msg.error);
          notifyFault(new Error(`worker fatal error: ${msg.error}`));
          break;
        case "frameError":
          console.error("[truapi worker]", msg.error);
          notifyFault(new Error(`worker frame error: ${msg.error}`));
          break;
        case "disposeError":
          console.warn("[truapi worker] dispose:", msg.error);
          break;
        case "frame":
          if (debugLoggingEnabled(state)) {
            console.debug("[truapi worker] frame <-", bytesToHex(msg.bytes));
          }
          for (const listener of [...state.listeners]) listener(msg.bytes);
          break;
        case "disconnectSessionResponse":
          handleDisconnectResponse(state, msg);
          break;
        case "permissionAuthorizationStatusResponse":
          handlePermissionAuthorizationStatusResponse(state, msg);
          break;
        case "setPermissionAuthorizationStatusResponse":
          handleSetPermissionAuthorizationStatusResponse(state, msg);
          break;
        case "callbackRequest":
          if (debugLoggingEnabled(state)) {
            console.debug("[truapi worker] callbackRequest", msg.name);
          }
          handleCallbackRequest(state, msg);
          break;
        case "subscriptionStart":
          handleSubscriptionStart(state, msg);
          break;
        case "subscriptionStop":
          handleSubscriptionStop(state, msg);
          break;
        case "chainConnectStart":
          if (debugLoggingEnabled(state)) {
            console.debug("[truapi worker] chainConnectStart", msg.connId);
          }
          void handleChainConnectStart(state, msg);
          break;
        case "chainSend":
          handleChainSend(state, msg);
          break;
        case "chainClose":
          handleChainClose(state, msg);
          break;
        default: {
          const { kind } = msg as { kind?: unknown };
          console.warn(
            `[truapi worker] unknown worker message kind: ${String(kind)}`,
          );
        }
      }
    };

    const notifyFault = (error: Error): void => {
      teardown(state, error, true);
    };

    const onError = (e: ErrorEvent): void => {
      cleanupInit();
      worker.terminate();
      reject(new Error(`worker init failed: ${e.message}`));
    };

    const onInitMessageError = (): void => {
      cleanupInit();
      worker.terminate();
      reject(new Error("worker message could not be deserialized during init"));
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
          logLevel: devLogLevelOverride ?? options.logLevel ?? "off",
          runtimeConfig: options.runtimeConfig,
          optionalCallbacks: implementedOptionalCallbacks(host),
          optionalSubscriptions: implementedOptionalSubscriptions(host),
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
        const provider = buildProvider(state);
        exposeDevGlobal(provider);
        resolve(provider);
      } else if (msg.kind === "fatalError") {
        cleanupInit();
        worker.terminate();
        reject(new Error(`worker init reported error: ${msg.error}`));
      }
    };

    const cleanupInit = (): void => {
      clearTimeout(initTimeout);
      worker.removeEventListener("error", onError);
      worker.removeEventListener("messageerror", onInitMessageError);
      worker.removeEventListener("message", onInitMessage);
    };

    const timeoutMs = options.initTimeoutMs ?? 30_000;
    const initTimeout = setTimeout(() => {
      cleanupInit();
      worker.terminate();
      reject(new Error(`worker init timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    worker.addEventListener("error", onError);
    worker.addEventListener("messageerror", onInitMessageError);
    worker.addEventListener("message", onInitMessage);
  });
}

function buildProvider(state: WorkerProviderState): TrUApiHostCoreProvider {
  const provider: TrUApiHostCoreProvider = {
    postMessage(bytes: Uint8Array): void {
      if (state.disposed) return;
      const post: MainToWorker = { kind: "frame", bytes };
      if (debugLoggingEnabled(state)) {
        console.debug("[truapi worker] frame ->", bytesToHex(bytes));
      }
      state.worker.postMessage(post);
    },
    subscribe(callback) {
      state.listeners.add(callback);
      return () => {
        state.listeners.delete(callback);
      };
    },
    subscribeClose(callback) {
      if (state.closedError) {
        callback(state.closedError);
        return () => {};
      }
      state.closeListeners.add(callback);
      return () => {
        state.closeListeners.delete(callback);
      };
    },
    disconnectSession(): Promise<void> {
      if (state.disposed) return Promise.resolve();
      return new Promise((resolve, reject) => {
        const requestId = ++nextDisconnectRequestId;
        state.pendingDisconnects.set(requestId, { resolve, reject });
        try {
          const post: MainToWorker = { kind: "disconnectSession", requestId };
          state.worker.postMessage(post);
        } catch (err) {
          state.pendingDisconnects.delete(requestId);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    },
    cancelPairing(): void {
      if (state.disposed) return;
      const post: MainToWorker = { kind: "cancelPairing" };
      state.worker.postMessage(post);
    },
    notifySessionStoreChanged(): void {
      if (state.disposed) return;
      const post: MainToWorker = { kind: "notifySessionStoreChanged" };
      state.worker.postMessage(post);
    },
    getPermissionAuthorizationStatus(
      request: PermissionAuthorizationRequest,
    ): Promise<PermissionAuthorizationStatus> {
      if (state.disposed) return Promise.resolve("NotDetermined");
      return new Promise((resolve, reject) => {
        const requestId = ++nextPermissionAuthorizationRequestId;
        state.pendingPermissionAuthorizationStatuses.set(requestId, {
          resolve,
          reject,
        });
        try {
          const post: MainToWorker = {
            kind: "getPermissionAuthorizationStatus",
            requestId,
            request: encodePermissionAuthorizationRequest(request),
          };
          state.worker.postMessage(post);
        } catch (err) {
          state.pendingPermissionAuthorizationStatuses.delete(requestId);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    },
    setPermissionAuthorizationStatus(
      request: PermissionAuthorizationRequest,
      status: PermissionAuthorizationStatus,
    ): Promise<void> {
      if (state.disposed) return Promise.resolve();
      return new Promise((resolve, reject) => {
        const requestId = ++nextPermissionAuthorizationRequestId;
        state.pendingSetPermissionAuthorizationStatuses.set(requestId, {
          resolve,
          reject,
        });
        try {
          const post: MainToWorker = {
            kind: "setPermissionAuthorizationStatus",
            requestId,
            request: encodePermissionAuthorizationRequest(request),
            status,
          };
          state.worker.postMessage(post);
        } catch (err) {
          state.pendingSetPermissionAuthorizationStatuses.delete(requestId);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    },
    setLogLevel(level: LogLevel): void {
      if (state.disposed) return;
      state.logLevel = level;
      const post: MainToWorker = { kind: "setLogLevel", level };
      state.worker.postMessage(post);
    },
    dispose() {
      devGlobalProviders.delete(provider);
      teardown(state, new Error("provider disposed"), false);
    },
  };
  return provider;
}

/**
 * Publish `globalThis.__truapi.setLogLevel(level)` so a developer can re-tune
 * the wasm core's verbosity live from the browser console without a reload. The
 * level is persisted to `localStorage["truapi:logLevel"]` and re-applied on the
 * next load, so it survives refreshes. Pair with the DevTools console "Verbose"
 * level to surface debug/trace.
 */
function exposeDevGlobal(provider: TrUApiHostCoreProvider): void {
  devGlobalProviders.add(provider);
  if (devLogLevelOverride !== null) {
    provider.setLogLevel?.(devLogLevelOverride);
  }
  publishDevGlobal();
}

function publishDevGlobal(): void {
  const target = globalThis as {
    __truapi?: TrUApiDevConsole;
  };
  target.__truapi = {
    setLogLevel(level: LogLevel): void {
      devLogLevelOverride = level;
      persistLogLevel(level);
      for (const provider of [...devGlobalProviders]) {
        provider.setLogLevel?.(level);
      }
      console.info(`[truapi worker] logLevel=${level}`);
    },
    getLogLevel(): LogLevel | null {
      return devLogLevelOverride;
    },
  };
}

publishDevGlobal();
