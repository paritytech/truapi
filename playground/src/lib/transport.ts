import {
  createClient,
  createMessagePortProvider,
  createTransport,
  type Provider,
  type TrUApiClient,
  type TrUApiTransport,
  type V01RemoteChainHeadFollowItem,
} from "@parity/truapi";

// `Hex` was a Vec<u8> alias in earlier generated TS; the new codegen
// inlines the underlying type, so we keep a local alias to avoid churning
// the rest of this module.
type Hex = Uint8Array;
type ChainHeadEvent = V01RemoteChainHeadFollowItem;

export type ConnectionStatus = "disconnected" | "connecting" | "connected";

declare global {
  interface Window {
    __HOST_WEBVIEW_MARK__?: boolean;
    __HOST_API_PORT__?: MessagePort;
  }
}

function isIframe(): boolean {
  try {
    return window !== window.top;
  } catch {
    return false;
  }
}

function isWebview(): boolean {
  return window.__HOST_WEBVIEW_MARK__ === true;
}

function isCorrectEnvironment(): boolean {
  return isIframe() || isWebview();
}

async function waitForWebviewPort(
  signal?: AbortSignal,
  timeoutMs = 20_000,
): Promise<MessagePort> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (signal?.aborted) throw new Error("waitForWebviewPort aborted");
    if (window.__HOST_API_PORT__) return window.__HOST_API_PORT__;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(
    `Timed out waiting for window.__HOST_API_PORT__ (${timeoutMs}ms)`,
  );
}

/** Origin used as the `targetOrigin` argument for outbound `postMessage`
 * frames. `document.referrer` is the URL of the parent document that loaded
 * the iframe, so its origin is the host that's expected to receive our
 * frames. We refuse to send if we can't pin a concrete origin (rather than
 * falling back to `"*"`), since the frames carry signed payloads and account
 * ids that must not leak to an unrelated frame parent. */
function resolveHostOrigin(): string | null {
  if (typeof document === "undefined" || !document.referrer) return null;
  try {
    return new URL(document.referrer).origin;
  } catch {
    return null;
  }
}

function createIframeProvider(): Provider {
  type MessageListener = (msg: Uint8Array) => void;
  type CloseListener = (error: Error) => void;
  const listeners = new Set<MessageListener>();
  const closeListeners = new Set<CloseListener>();
  let disposed = false;

  const parent = window.parent;
  if (!parent) {
    throw new Error("Iframe provider requires a parent window");
  }

  const hostOrigin = resolveHostOrigin();
  if (!hostOrigin) {
    throw new Error(
      "Iframe provider could not resolve the host origin from document.referrer; " +
        "the playground must be embedded by a host that sends a Referer header.",
    );
  }

  const onMessage = (event: MessageEvent) => {
    if (disposed) return;
    if (event.source !== parent) return;
    if (event.origin !== hostOrigin) return;
    if (!(event.data instanceof Uint8Array)) return;
    for (const listener of listeners) listener(event.data);
  };

  window.addEventListener("message", onMessage);

  return {
    postMessage(message: Uint8Array) {
      if (disposed) throw new Error("iframe provider disposed");
      // Pin the target origin so SCALE-encoded frames carrying signed
      // payloads / account ids don't leak to an unrelated frame parent.
      parent.postMessage(message, hostOrigin);
    },
    subscribe(callback: MessageListener) {
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },
    subscribeClose(callback: CloseListener) {
      closeListeners.add(callback);
      return () => {
        closeListeners.delete(callback);
      };
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      window.removeEventListener("message", onMessage);
      const error = new Error("iframe provider disposed");
      for (const listener of closeListeners) listener(error);
      listeners.clear();
      closeListeners.clear();
    },
  };
}

function createSandboxProvider(): Provider {
  if (isIframe()) return createIframeProvider();
  if (isWebview()) {
    const portController = new AbortController();
    const provider = createMessagePortProvider(
      waitForWebviewPort(portController.signal),
    );
    const baseDispose = provider.dispose;
    provider.dispose = () => {
      portController.abort();
      baseDispose?.();
    };
    return provider;
  }
  throw new Error(
    "Playground must be opened inside a TrUAPI host (iframe or webview); detected neither.",
  );
}

let _provider: Provider | null = null;
let _transport: TrUApiTransport | null = null;
let _client: TrUApiClient | null = null;
let _status: ConnectionStatus = "disconnected";
const _statusListeners = new Set<(status: ConnectionStatus) => void>();

function setStatus(next: ConnectionStatus) {
  if (_status === next) return;
  _status = next;
  for (const listener of _statusListeners) listener(next);
}

function ensureClient(): TrUApiClient {
  if (_client) return _client;
  if (!isCorrectEnvironment()) {
    throw new Error("Playground must be opened inside a TrUAPI host");
  }
  _provider = createSandboxProvider();
  _provider.subscribeClose?.(() => setStatus("disconnected"));
  // `createTransport` auto-responds to inbound `host_handshake_request`
  // frames with the versioned response variant requested by the host; see
  // the matching comment in @parity/truapi/client.ts.
  _transport = createTransport(_provider);
  _client = createClient(_transport);
  return _client;
}

export function getClient(): TrUApiClient {
  return ensureClient();
}

export function getTransport(): TrUApiTransport {
  ensureClient();
  return _transport!;
}

const HANDSHAKE_TIMEOUT_MS = 5_000;

/** Subscribe to connection status changes; kicks off a handshake the first time. */
export function subscribeConnectionStatus(
  callback: (status: ConnectionStatus) => void,
): () => void {
  _statusListeners.add(callback);
  callback(_status);

  if (_status === "disconnected") {
    if (!isCorrectEnvironment()) {
      // Standalone (not iframed, not webview): no host to talk to. Stay
      // disconnected so the UI surfaces the OFFLINE chip and method bindings
      // gracefully refuse to bind.
      return () => {
        _statusListeners.delete(callback);
      };
    }
    setStatus("connecting");
    try {
      const handshake = ensureClient()
        .trUApiCalls.handshake()
        .then((result) => result.isOk());
      const timeout = new Promise<boolean>((resolve) =>
        setTimeout(() => resolve(false), HANDSHAKE_TIMEOUT_MS),
      );
      void Promise.race([handshake, timeout])
        .then((success: boolean) =>
          setStatus(success ? "connected" : "disconnected"),
        )
        .catch(() => setStatus("disconnected"));
    } catch {
      setStatus("disconnected");
    }
  }

  return () => {
    _statusListeners.delete(callback);
  };
}

export { isCorrectEnvironment };

// ---------------------------------------------------------------------------
// chain_head_follow helpers
//
// `remote_chain_head_*` dependent calls (header, body, storage, call, unpin,
// continue, stop_operation) are scoped to a live `chain_head_follow`
// subscription. The bridge auto-opens an ephemeral follow when the caller
// leaves `followSubscriptionId` empty, fills in the subscription id from
// `Subscription.subscriptionId`, and unsubscribes once the dependent call
// (and any matching operation events) settle.
// ---------------------------------------------------------------------------

type ChainHeadEventListener = (event: ChainHeadEvent) => void;

export interface EphemeralFollow {
  subscriptionId: string;
  genesisHash: Hex;
  finalizedBlockHash: Hex;
  /** Subscribe to subsequent ChainHeadEvents on this follow. */
  onEvent: (listener: ChainHeadEventListener) => () => void;
  /** Send the stop frame and clear listeners. Idempotent. */
  unsubscribe: () => void;
}

export function hexToBytes(hex: string): Uint8Array {
  if (!/^0x[0-9a-fA-F]*$/.test(hex)) {
    throw new Error(`hexToBytes: not a hex-prefixed string: ${hex}`);
  }
  const body = hex.slice(2);
  const out = new Uint8Array(body.length / 2);
  for (let i = 0; i < body.length; i += 2) {
    out[i / 2] = parseInt(body.slice(i, i + 2), 16);
  }
  return out;
}

/** Opens a one-shot `chain_head_follow`, waits for the first `Initialized`
 * event, and resolves with the transport-assigned subscription id plus the
 * first finalized block hash. The follow stays alive until `unsubscribe()` is
 * called; the caller is expected to do so after the dependent call settles. */
export function openEphemeralFollow(
  genesisHash: Hex | string,
  withRuntime = false,
  timeoutMs = 15_000,
): Promise<EphemeralFollow> {
  const client = getClient();
  const listeners = new Set<ChainHeadEventListener>();
  const genesisHashBytes =
    typeof genesisHash === "string" ? hexToBytes(genesisHash) : genesisHash;

  return new Promise((resolve, reject) => {
    let settled = false;
    let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
    const sub = client.chainInteraction
      .chainHeadFollow({
        request: { genesisHash: genesisHashBytes, withRuntime },
      })
      .subscribe({
        next: (event) => {
          if (!settled) {
            if (event.tag !== "Initialized") return;
            const finalizedBlockHash = event.value.finalizedBlockHashes[0];
            if (!finalizedBlockHash) {
              settled = true;
              if (timeoutHandle !== null) clearTimeout(timeoutHandle);
              try {
                sub.unsubscribe();
              } catch {
                /* benign */
              }
              reject(
                new Error("Initialized event had no finalized block hash"),
              );
              return;
            }
            settled = true;
            if (timeoutHandle !== null) clearTimeout(timeoutHandle);
            resolve({
              subscriptionId: sub.subscriptionId,
              genesisHash: genesisHashBytes,
              finalizedBlockHash,
              onEvent: (listener) => {
                listeners.add(listener);
                return () => listeners.delete(listener);
              },
              unsubscribe: () => {
                try {
                  sub.unsubscribe();
                } catch {
                  /* benign */
                }
                listeners.clear();
              },
            });
            return;
          }
          for (const listener of listeners) listener(event);
        },
        error: (error) => {
          listeners.clear();
          if (settled) return;
          settled = true;
          if (timeoutHandle !== null) clearTimeout(timeoutHandle);
          reject(error);
        },
        complete: () => {
          listeners.clear();
          if (settled) return;
          settled = true;
          if (timeoutHandle !== null) clearTimeout(timeoutHandle);
          reject(new Error("chain_head_follow completed before Initialized"));
        },
      });
    timeoutHandle = setTimeout(() => {
      if (settled) return;
      settled = true;
      try {
        sub.unsubscribe();
      } catch {
        /* benign */
      }
      reject(
        new Error(
          `openEphemeralFollow: no Initialized event within ${timeoutMs}ms`,
        ),
      );
    }, timeoutMs);
  });
}

/** Wait for a chain-head operation event matching `operationId` whose tag is
 * one of `terminalTags`. Resolves with the matching event or rejects on
 * timeout. */
export function awaitChainHeadOperation(
  follow: EphemeralFollow,
  operationId: string,
  terminalTags: readonly string[],
  timeoutMs = 30_000,
): Promise<ChainHeadEvent> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
    const cleanup = follow.onEvent((event) => {
      if (settled) return;
      if (!terminalTags.includes(event.tag)) return;
      const eventOpId = (event.value as { operationId?: string } | undefined)
        ?.operationId;
      if (eventOpId !== operationId) return;
      settled = true;
      if (timeoutHandle !== null) clearTimeout(timeoutHandle);
      cleanup();
      resolve(event);
    });
    timeoutHandle = setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(
        new Error(
          `awaitChainHeadOperation: no terminal event for ${operationId} within ${timeoutMs}ms`,
        ),
      );
    }, timeoutMs);
  });
}

/** Storage variant: accumulates `OperationStorageItems` until a terminal
 * `OperationStorageDone` / `OperationError` / `OperationInaccessible` event.
 * `onWaitingForContinue` is fired when the host pauses delivery. */
export function awaitChainHeadStorage(
  follow: EphemeralFollow,
  operationId: string,
  options: {
    onWaitingForContinue?: () => void;
    timeoutMs?: number;
  } = {},
): Promise<{ items: unknown[]; done: ChainHeadEvent }> {
  const timeoutMs = options.timeoutMs ?? 30_000;
  const TERMINAL = [
    "OperationStorageDone",
    "OperationError",
    "OperationInaccessible",
  ];

  return new Promise((resolve, reject) => {
    let settled = false;
    const items: unknown[] = [];
    let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
    const cleanup = follow.onEvent((event) => {
      if (settled) return;
      const eventOpId = (event.value as { operationId?: string } | undefined)
        ?.operationId;
      if (eventOpId !== operationId) return;
      if (event.tag === "OperationStorageItems") {
        const evValue = event.value as { items?: unknown[] };
        if (Array.isArray(evValue.items)) items.push(...evValue.items);
        return;
      }
      if (event.tag === "OperationWaitingForContinue") {
        options.onWaitingForContinue?.();
        return;
      }
      if (TERMINAL.includes(event.tag)) {
        settled = true;
        if (timeoutHandle !== null) clearTimeout(timeoutHandle);
        cleanup();
        resolve({ items, done: event });
      }
    });
    timeoutHandle = setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(
        new Error(
          `awaitChainHeadStorage: no terminal event for ${operationId} within ${timeoutMs}ms`,
        ),
      );
    }, timeoutMs);
  });
}
