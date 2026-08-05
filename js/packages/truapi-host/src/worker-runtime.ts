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
import type { GenericError } from "@parity/truapi";
import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import {
  createWorkerRawCallbacks,
  type CallbackName,
} from "./generated/worker-callbacks.js";
import {
  handleGetPermissionAuthorizationStatus,
  handleGetPermissionAuthorizationStatuses,
  handleSetPermissionAuthorizationStatus,
  type PermissionAuthorizationRuntime,
} from "./worker-permission-authorization.js";
import { errorMessage } from "./error.js";
import {
  dispatchChainResponse,
  dispatchSubscriptionError,
  dispatchSubscriptionItem,
  type SubscriptionListeners,
} from "./worker-dispatch.js";

interface WorkerProductRuntime {
  receiveFrame(frame: Uint8Array): Promise<void>;
  dispose(): void;
  free(): void;
}

interface WorkerPairingHostRuntime extends PermissionAuthorizationRuntime {
  productRuntime(
    product: unknown,
    coreCallbacks: unknown,
  ): WorkerProductRuntime;
  disconnectSession(): Promise<void>;
  cancelPairing(): void;
  notifySessionStoreChanged(): void;
  free(): void;
}

interface WasmModuleShape {
  default: (input?: unknown) => Promise<unknown>;
  WasmPairingHostRuntime: new (
    callbacks: unknown,
    hostConfig: unknown,
  ) => WorkerPairingHostRuntime;
  WasmProductRuntime: new (
    callbacks: unknown,
    runtimeConfig: unknown,
  ) => WorkerProductRuntime;
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
const subscriptionListeners = new Map<number, SubscriptionListeners>();

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
  sendError: (error: GenericError) => void,
): () => void {
  const subId = ++nextSubId;
  subscriptionListeners.set(subId, {
    sendItem: sendItem as (value: unknown) => void,
    sendError: (error) => sendError({ reason: error }),
  });
  postToMain({ kind: "subscriptionStart", subId, name, payload });
  return () => {
    subscriptionListeners.delete(subId);
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

/** Build the host-level callback object passed to the WASM runtime. */
function buildRawCallbacks() {
  return createWorkerRawCallbacks({
    callbackRequest,
    startSubscription,
    chainConnect,
  });
}

/** Encode raw frame bytes as base64 (JSON can't carry binary over the WS). */
function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

/**
 * Dev-only link to the debugger the host dials. Fire-and-forget by construction:
 * it opens lazily, buffers a bounded backlog until the socket is up, retries a
 * dropped connection, and swallows every error - a slow, absent, or crashed
 * debugger only loses the trace, it can never throw into the frame path.
 */
/**
 * Envelope version stamped on each frame, mirroring the debugger's
 * `WIRE_ENVELOPE_VERSION`. Kept in sync by hand (a value constant, not a shared
 * dep, to avoid truapi-host depending on the debugger package).
 */
const WIRE_ENVELOPE_VERSION = 1;

/**
 * Is `url` a `ws://` URL on a loopback host? The debug tap forwards raw frames
 * (including sensitive payloads, before the debugger's denylist runs), so it is
 * loopback-only: refuse to stream them off the local machine. `ws://` only,
 * matching the native sink (`native_debug.rs`), which is also ws-only.
 */
export function isLoopbackWsUrl(url: string): boolean {
  try {
    const u = new URL(url);
    if (u.protocol !== "ws:") return false;
    const host = u.hostname.replace(/^\[|\]$/g, "").toLowerCase();
    return (
      host === "localhost" ||
      host === "::1" ||
      /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host) ||
      // IPv4-mapped loopback: WHATWG serializes ::ffff:127.x.y.z as ::ffff:7fxx:yyyy.
      /^::ffff:7f[0-9a-f]{2}:/.test(host)
    );
  } catch {
    return false;
  }
}

function createDebuggerLink(url: string): {
  emit(channelId: string, dir: string, frame: Uint8Array): void;
} {
  // Loopback-only, dev-only: a non-loopback (or non-ws://) debugger URL yields an
  // inert link rather than streaming frames across the network. Warn so a
  // mistyped value reads as "misconfigured", not "the debugger doesn't work".
  if (!isLoopbackWsUrl(url)) {
    console.warn(
      `[truapi] wire debugger URL rejected (must be ws:// on a loopback host): ${url}`,
    );
    return { emit() {} };
  }
  let socket: WebSocket | null = null;
  let open = false;
  const queue: string[] = [];
  // Count *and* byte caps: each queued item is a base64 ProtocolMessage (storage
  // writes, RPC responses - up to MBs each), so a count-only cap would let a slow
  // or absent debugger buffer unbounded RSS on the observed session. Whichever
  // ceiling hits first drops the frame (counted), never blocking the frame path.
  const MAX_QUEUE = 1000;
  const MAX_QUEUE_BYTES = 8 * 1024 * 1024;
  let queuedBytes = 0;
  let droppedSinceSend = 0;

  function connect(): void {
    try {
      socket = new WebSocket(url);
    } catch {
      socket = null;
      return;
    }
    socket.addEventListener("open", () => {
      open = true;
      const pending = queue.splice(0);
      queuedBytes = 0;
      // Deliver drops accumulated while disconnected by stamping the count on the
      // first drained frame - a bare marker without channelId/dir/frame wouldn't
      // parse server-side. Drops only happen once the queue is full, so when the
      // count is nonzero there is always a pending frame to carry it; if not, it
      // rides the next live emit.
      if (pending.length > 0 && droppedSinceSend > 0) {
        try {
          const first = JSON.parse(pending[0]) as Record<string, unknown>;
          first.dropped = droppedSinceSend;
          pending[0] = JSON.stringify(first);
          droppedSinceSend = 0;
        } catch {
          // Leave the frame as-is; the count rides the next live emit.
        }
      }
      for (const message of pending) send(message);
    });
    socket.addEventListener("close", () => {
      open = false;
      socket = null;
    });
    socket.addEventListener("error", () => {
      // A socket that fired `error` is dead: close it explicitly (tidiness), then
      // null it so `emit`'s `if (!socket) connect()` reconnects. Without the null,
      // a runtime that fires `error` without a following `close` would leave
      // `socket` non-null and frames would buffer then drop.
      open = false;
      const dead = socket;
      socket = null;
      try {
        dead?.close();
      } catch {
        // already closed / closing
      }
    });
  }

  function send(message: string): void {
    try {
      socket?.send(message);
    } catch {
      // A dead socket must never break the frame path.
    }
  }

  connect();

  let warnedDrop = false;
  return {
    emit(channelId, dir, frame) {
      // A debug tap must never throw into the observed frame path: toBase64 /
      // JSON.stringify can raise on a pathological frame (btoa or V8 string-length
      // limits), and only send() swallows its own errors. Losing a trace is fine;
      // breaking dispatch is not.
      try {
        const base = {
          v: WIRE_ENVELOPE_VERSION,
          codec: TRUAPI_CODEC_VERSION,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          channelId,
          dir,
          frame: toBase64(frame),
        };
        if (open && socket) {
          // Piggyback any frames dropped while the link was down onto the next
          // live frame, so the debugger attributes the gap to the link, not the
          // host.
          send(
            droppedSinceSend > 0
              ? JSON.stringify({ ...base, dropped: droppedSinceSend })
              : JSON.stringify(base),
          );
          droppedSinceSend = 0;
          return;
        }
        const message = JSON.stringify(base);
        if (
          queue.length < MAX_QUEUE &&
          queuedBytes + message.length <= MAX_QUEUE_BYTES
        ) {
          queue.push(message);
          queuedBytes += message.length;
        } else {
          droppedSinceSend += 1;
          if (!warnedDrop) {
            // The link buffers a bounded backlog while the debugger is
            // absent/slow; once full (by count or bytes), frames are dropped.
            // Warn once so the gap is attributable to the link, not the host.
            warnedDrop = true;
            console.warn(
              "[truapi] wire debugger link queue full — dropping frames until it drains",
            );
          }
        }
        if (!socket) connect();
      } catch {
        // Swallow: never let the tap disturb the frame path.
      }
    },
  };
}

let debuggerLink: ReturnType<typeof createDebuggerLink> | null = null;

function buildCoreCallbacks(coreId: number) {
  const callbacks = {
    emitFrame(frame: Uint8Array): void {
      postToMain({ kind: "frame", coreId, bytes: frame });
    },
    dispose(): void {
      // Main thread owns lifecycle and disposes explicitly.
    },
  };
  if (!debuggerLink) return callbacks;
  // Adding `debugEmit` is what makes the Rust host install its debug sink; when
  // no debugger is configured it is absent and the tap stays inert.
  return {
    ...callbacks,
    debugEmit(channelId: string, dir: string, frame: Uint8Array): void {
      debuggerLink?.emit(channelId, dir, frame);
    },
  };
}

let runtime: WorkerPairingHostRuntime | null = null;
const cores = new Map<number, WorkerProductRuntime>();
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
      if (runtime) {
        postToMain({
          kind: "fatalError",
          error: "init: runtime already initialized",
        });
        break;
      }
      wasm.setLogLevel?.(msg.logLevel);
      if (msg.debuggerUrl && !debuggerLink) {
        debuggerLink = createDebuggerLink(msg.debuggerUrl);
      }
      try {
        runtime = new wasm.WasmPairingHostRuntime(
          buildRawCallbacks(),
          msg.hostConfig,
        );
        postToMain({ kind: "ready" });
      } catch (err) {
        postToMain({ kind: "fatalError", error: `init: ${errorMessage(err)}` });
      }
      break;
    case "createCore":
      if (!runtime) {
        postToMain({
          kind: "coreError",
          coreId: msg.coreId,
          error: "createCore received before runtime is ready",
        });
        break;
      }
      try {
        const core = runtime.productRuntime(
          msg.product,
          buildCoreCallbacks(msg.coreId),
        );
        cores.set(msg.coreId, core);
        postToMain({ kind: "coreReady", coreId: msg.coreId });
      } catch (err) {
        postToMain({
          kind: "coreError",
          coreId: msg.coreId,
          error: errorMessage(err),
        });
      }
      break;
    case "setLogLevel":
      wasm?.setLogLevel?.(msg.level);
      break;
    case "frame":
      void handleFrame(msg.coreId, msg.bytes);
      break;
    case "disconnectSession":
      void handleDisconnectSession(msg.requestId);
      break;
    case "cancelPairing":
      runtime?.cancelPairing();
      break;
    case "notifySessionStoreChanged":
      runtime?.notifySessionStoreChanged();
      break;
    case "getPermissionAuthorizationStatus":
      void handleGetPermissionAuthorizationStatus(
        runtime,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.request,
      );
      break;
    case "getPermissionAuthorizationStatuses":
      void handleGetPermissionAuthorizationStatuses(
        runtime,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.requests,
      );
      break;
    case "setPermissionAuthorizationStatus":
      void handleSetPermissionAuthorizationStatus(
        runtime,
        postToMain,
        msg.productId,
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
      dispatchSubscriptionItem(
        msg.subId,
        msg.value,
        subscriptionListeners,
        postToMain,
      );
      break;
    }
    case "subscriptionError": {
      dispatchSubscriptionError(
        msg.subId,
        msg.error,
        subscriptionListeners,
        postToMain,
      );
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
      dispatchChainResponse(
        msg.connId,
        msg.json,
        chainResponseListeners,
        postToMain,
      );
      break;
    }
    case "disposeCore":
      disposeCore(msg.coreId);
      break;
    case "dispose":
      try {
        for (const coreId of [...cores.keys()]) {
          disposeCore(coreId);
        }
        runtime?.free();
      } catch (err) {
        postToMain({ kind: "disposeError", error: errorMessage(err) });
      }
      runtime = null;
      break;
    default: {
      const { kind } = msg as { kind?: unknown };
      console.warn(
        `[truapi worker-runtime] unknown message kind: ${String(kind)}`,
      );
    }
  }
});

function disposeCore(coreId: number): void {
  const core = cores.get(coreId);
  if (!core) return;
  cores.delete(coreId);
  try {
    core.dispose();
    core.free();
  } catch (err) {
    postToMain({ kind: "disposeError", error: errorMessage(err) });
  }
}

async function handleDisconnectSession(requestId: number): Promise<void> {
  if (!runtime) {
    postToMain({
      kind: "disconnectSessionResponse",
      requestId,
      ok: false,
      error: "disconnectSession received before runtime is ready",
    });
    return;
  }
  try {
    await runtime.disconnectSession();
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

async function handleFrame(coreId: number, bytes: Uint8Array): Promise<void> {
  const core = cores.get(coreId);
  if (!core) {
    postToMain({
      kind: "frameError",
      coreId,
      error: `frame received for unknown core ${coreId}`,
    });
    return;
  }
  try {
    await core.receiveFrame(bytes);
  } catch (err) {
    postToMain({
      kind: "frameError",
      coreId,
      error: errorMessage(err),
    });
  }
}
