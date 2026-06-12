import type {
  ChainConnection,
  HostCallbacks,
  LogLevel,
  MainToWorker,
  SubscriptionName,
  TrUApiHostWasmProvider,
  WasmRuntimeConfig,
  WasmRawCallbacks,
  WorkerToMain,
} from "../index.js";
import { createWasmRawCallbacks } from "../generated/host-callbacks-adapter.js";
import {
  OPTIONAL_CALLBACK_NAMES,
  type CallbackName,
  type OptionalCallbackName,
} from "../worker-protocol.js";
import { errorMessage } from "../error-message.js";
import {
  SUBSCRIPTION_DISPATCH,
  subscriptionDispatchEntry,
} from "../subscription-table.js";
import { decodeWireMessage, describeWireId } from "@parity/truapi";
import { bytesToHex } from "@parity/truapi/scale";

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
  closedError: Error | null;
  logLevel: LogLevel;
  disposed: boolean;
}

function debugLoggingEnabled(state: WorkerProviderState): boolean {
  return state.logLevel === "debug" || state.logLevel === "trace";
}

let nextDisconnectRequestId = 0;

/** localStorage key the dev log level is persisted under, so it survives reloads. */
const DEV_LOG_LEVEL_KEY = "truapi:logLevel";
const LOG_LEVELS: readonly LogLevel[] = [
  "off",
  "error",
  "warn",
  "info",
  "debug",
  "trace",
];

function isLogLevel(value: string | null): value is LogLevel {
  return value !== null && (LOG_LEVELS as readonly string[]).includes(value);
}

/** Read the persisted dev log level. Returns null when unset or unavailable. */
function readPersistedLogLevel(): LogLevel | null {
  try {
    const stored = globalThis.localStorage?.getItem(DEV_LOG_LEVEL_KEY);
    return isLogLevel(stored) ? stored : null;
  } catch {
    return null;
  }
}

/** Persist the dev log level so it re-applies on the next reload. */
function persistLogLevel(level: LogLevel): void {
  try {
    globalThis.localStorage?.setItem(DEV_LOG_LEVEL_KEY, level);
  } catch {
    // Storage unavailable (sandboxed iframe / privacy mode); the level still
    // applies for the current session.
  }
}

let devLogLevelOverride: LogLevel | null = readPersistedLogLevel();
const devGlobalProviders = new Set<TrUApiHostWasmProvider>();

interface TrUApiDevConsole {
  setLogLevel(level: LogLevel): void;
  getLogLevel(): LogLevel | null;
  getProviderCount(): number;
}

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
  return SUBSCRIPTION_DISPATCH.filter(
    ({ callback }) => typeof callbacks[callback] === "function",
  ).map(({ protocol }) => protocol);
}

function bytesToHexPreview(bytes: Uint8Array, maxBytes = 96): string {
  const visible = bytes.subarray(0, maxBytes);
  const suffix =
    bytes.length > maxBytes ? `…(+${bytes.length - maxBytes})` : "";
  return `${bytesToHex(visible)}${suffix}`;
}

function describeWireFrame(bytes: Uint8Array) {
  const decoded = decodeWireMessage(bytes);
  if (decoded.isErr()) {
    return {
      frameBytes: bytes.byteLength,
      decodeError: decoded.error.message,
      frameHex: bytesToHexPreview(bytes),
    };
  }
  const wireId = decoded.value.payload.id;
  const payload = decoded.value.payload.value;
  return {
    frame: describeWireId(wireId),
    requestId: decoded.value.requestId,
    wireId,
    frameBytes: bytes.byteLength,
    payloadBytes: payload.byteLength,
    payloadHex: bytesToHexPreview(payload),
  };
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
  const entry = subscriptionDispatchEntry(msg.name);
  if (!entry) {
    console.warn(`[truapi worker] unknown subscription: ${msg.name}`);
    return;
  }
  const sendItem = (value?: unknown): void => {
    if (state.disposed) return;
    const post: MainToWorker = {
      kind: "subscriptionItem",
      subId: msg.subId,
      value,
    };
    state.worker.postMessage(post);
  };
  let dispose: (() => void) | void;
  try {
    if (entry.payload === "required") {
      if (msg.payload === null) {
        console.warn(
          `[truapi worker] ${msg.name} requires payload, none received`,
        );
        return;
      }
      dispose = entry.start(state.rawCallbacks, msg.payload, sendItem);
    } else {
      dispose = entry.start(state.rawCallbacks, sendItem);
    }
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

function rejectPendingDisconnects(
  state: WorkerProviderState,
  error: Error,
): void {
  for (const pending of state.pendingDisconnects.values()) {
    pending.reject(error);
  }
  state.pendingDisconnects.clear();
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
  runtimeConfig: WasmRuntimeConfig;
  /**
   * Milliseconds to wait for the worker to report `ready` before rejecting
   * and terminating it. Default: 30000.
   */
  initTimeoutMs?: number;
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
): Promise<TrUApiHostWasmProvider> {
  if (!options?.runtimeConfig) {
    return Promise.reject(new Error("runtimeConfig is required"));
  }
  const callbacks = createWasmRawCallbacks(host);

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
            console.debug(
              "[truapi worker] frame <-",
              describeWireFrame(msg.bytes),
            );
          }
          for (const listener of [...state.listeners]) listener(msg.bytes);
          break;
        case "disconnectResponse":
          handleDisconnectResponse(state, msg);
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

function buildProvider(state: WorkerProviderState): TrUApiHostWasmProvider {
  const provider: TrUApiHostWasmProvider = {
    postMessage(bytes: Uint8Array): void {
      if (state.disposed) return;
      const post: MainToWorker = { kind: "frame", bytes };
      if (debugLoggingEnabled(state)) {
        console.debug("[truapi worker] frame ->", describeWireFrame(bytes));
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
    cancelLogin(): void {
      if (state.disposed) return;
      const post: MainToWorker = { kind: "cancelLogin" };
      state.worker.postMessage(post);
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
function exposeDevGlobal(provider: TrUApiHostWasmProvider): void {
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
      console.info(
        `[truapi worker] logLevel=${level} providers=${String(devGlobalProviders.size)}`,
      );
    },
    getLogLevel(): LogLevel | null {
      return devLogLevelOverride;
    },
    getProviderCount(): number {
      return devGlobalProviders.size;
    },
  };
}

publishDevGlobal();
