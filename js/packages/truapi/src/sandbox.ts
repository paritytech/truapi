/**
 * Sandbox bootstrap for browser-embedded hosts.
 *
 * Detects whether the app runs inside a TrUAPI host (iframe or webview), builds
 * the matching {@link WireProvider}, and exposes a lazily-created, cached
 * {@link TrUApiClient} via {@link getClientSync} so embedders don't
 * re-implement the wiring. {@link subscribeConnectionStatus} surfaces a
 * connected / disconnected signal over the same cached client.
 *
 * @module
 */

import {
  createMessagePortProvider,
  type WireProvider,
} from "./transport.js";
import { createTransport } from "./client.js";
import { createClient, type TrUApiClient } from "./generated/index.js";

/**
 * Connection lifecycle state. {@link subscribeConnectionStatus} emits
 * `"connected"` / `"disconnected"`; `"connecting"` is reserved for consumers
 * that want to render an indeterminate state before the first status is known.
 */
export type ConnectionStatus = "disconnected" | "connecting" | "connected";

declare global {
  interface Window {
    /** Set by webview hosts (Polkadot Desktop / Mobile) to mark the embedding. */
    __HOST_WEBVIEW_MARK__?: boolean;
    /** Injected by webview hosts to carry the host-side `MessagePort`. */
    __HOST_API_PORT__?: MessagePort;
  }
}

function hostWindow(): Window | null {
  return typeof window === "undefined" ? null : window;
}

function isIframe(): boolean {
  try {
    return window !== window.top;
  } catch {
    // A cross-origin parent throws on access, which itself means we're embedded.
    return true;
  }
}

/**
 * Detect whether the app is running inside a TrUAPI host container: an iframe
 * (including a cross-origin parent), a marked webview, or a window carrying an
 * injected host message port. Synchronous, so it can gate hot paths.
 */
export function isCorrectEnvironment(): boolean {
  const win = hostWindow();
  if (!win) return false;
  if (isIframe()) return true;
  if (win.__HOST_WEBVIEW_MARK__ === true) return true;
  if (win.__HOST_API_PORT__ != null) return true;
  return false;
}

/**
 * Origin used as the `targetOrigin` for iframe bootstrap messages.
 */
function resolveHostOrigin(): string | null {
  if (typeof document !== "undefined" && document.referrer) {
    try {
      return new URL(document.referrer).origin;
    } catch {
      // Fall through to ancestorOrigins.
    }
  }
  const ancestors = window.location?.ancestorOrigins;
  if (ancestors && ancestors.length > 0) return ancestors[0] ?? null;
  return null;
}

const HOST_PORT_TIMEOUT_MS = 20_000;
let iframePortPromise: Promise<MessagePort> | null = null;

/**
 * Resolve the host-injected `MessagePort`, polling `window.__HOST_API_PORT__`
 * until it appears or the timeout elapses. Rejects on timeout or abort.
 *
 * TODO(cleanup): this polling is defensive against the port not being injected
 * yet. Once hosts guarantee the iframe / `__HOST_API_PORT__` is wired before any
 * product code runs, read `window.__HOST_API_PORT__` directly and drop the
 * poll loop + timeout.
 */
async function waitForWebviewPort(
  signal?: AbortSignal,
  timeoutMs = HOST_PORT_TIMEOUT_MS,
): Promise<MessagePort> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (signal?.aborted) throw new Error("waitForWebviewPort aborted");
    const port = hostWindow()?.__HOST_API_PORT__;
    if (port) return port;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(
    `Timed out waiting for window.__HOST_API_PORT__ (${timeoutMs}ms)`,
  );
}

/**
 * Resolve the iframe `MessagePort` transferred by `createIframeHost`.
 */
function waitForIframePort(
  signal?: AbortSignal,
  timeoutMs = HOST_PORT_TIMEOUT_MS,
): Promise<MessagePort> {
  const existing = hostWindow()?.__HOST_API_PORT__;
  if (existing) return Promise.resolve(existing);
  if (iframePortPromise) return iframePortPromise;

  iframePortPromise = new Promise<MessagePort>((resolve, reject) => {
    const win = hostWindow();
    if (!win) {
      reject(new Error("window is unavailable"));
      return;
    }

    const hostOrigin = resolveHostOrigin();
    let done = false;
    const cleanup = (): void => {
      win.removeEventListener("message", onMessage);
      signal?.removeEventListener("abort", onAbort);
      clearTimeout(timer);
    };
    const finish = (result: MessagePort | Error): void => {
      if (done) return;
      done = true;
      cleanup();
      if (result instanceof Error) {
        reject(result);
      } else {
        win.__HOST_API_PORT__ = result;
        resolve(result);
      }
    };
    const onAbort = (): void => {
      finish(new Error("waitForIframePort aborted"));
    };
    const onMessage = (event: MessageEvent): void => {
      if (event.source !== win.parent) return;
      if (
        hostOrigin !== null &&
        event.origin !== hostOrigin &&
        event.origin !== "null"
      ) {
        return;
      }
      if (event.data?.type !== "truapi-init") return;
      const [port] = event.ports;
      if (!port) {
        finish(new Error("truapi-init did not include a MessagePort"));
        return;
      }
      finish(port);
    };
    const timer = setTimeout(() => {
      finish(
        new Error(`Timed out waiting for iframe MessagePort (${timeoutMs}ms)`),
      );
    }, timeoutMs);

    win.addEventListener("message", onMessage);
    signal?.addEventListener("abort", onAbort, { once: true });
    win.parent.postMessage({ type: "truapi-ready" }, hostOrigin ?? "*");
  }).catch((error: unknown) => {
    iframePortPromise = null;
    throw error;
  });

  return iframePortPromise;
}

/** Build the {@link WireProvider} matching the detected environment (iframe or webview). */
function createSandboxProvider(): WireProvider {
  const portController = new AbortController();
  if (isIframe()) {
    const provider = createMessagePortProvider(
      waitForIframePort(portController.signal),
    );
    const baseDispose = provider.dispose;
    provider.dispose = () => {
      portController.abort();
      baseDispose?.();
    };
    return provider;
  }
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

let cachedClient: TrUApiClient | null = null;
let status: ConnectionStatus = "disconnected";
const statusListeners = new Set<(status: ConnectionStatus) => void>();

function setStatus(next: ConnectionStatus): void {
  if (status === next) return;
  status = next;
  for (const listener of statusListeners) listener(next);
}

/**
 * Build (or return the cached) {@link TrUApiClient}. Returns `null` outside a
 * host container or if the provider can't be built.
 */
export function getClientSync(): TrUApiClient | null {
  if (cachedClient) return cachedClient;
  if (!isCorrectEnvironment()) return null;
  try {
    const provider = createSandboxProvider();
    cachedClient = createClient(createTransport(provider));
    provider.subscribeClose?.(() => setStatus("disconnected"));
    return cachedClient;
  } catch {
    return null;
  }
}

/**
 * Subscribe to connection-status changes. The callback fires immediately with
 * the current status and on every transition. Status is `"connected"` once the
 * client is built inside a host container, and `"disconnected"` otherwise (or
 * when the provider reports the pipe closed). Returns an unsubscribe function.
 */
export function subscribeConnectionStatus(
  callback: (status: ConnectionStatus) => void,
): () => void {
  let emitted = false;
  const listener = (next: ConnectionStatus) => {
    emitted = true;
    callback(next);
  };
  statusListeners.add(listener);

  if (status === "disconnected") {
    setStatus(getClientSync() ? "connected" : "disconnected");
  }
  if (!emitted) {
    callback(status);
  }

  return () => {
    statusListeners.delete(listener);
  };
}
