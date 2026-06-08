import {
  createClient,
  createMessagePortProvider,
  createTransport,
  type Provider,
  type TrUApiClient,
  type TrUApiTransport,
} from "@parity/truapi";

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

function waitForIframePort(
  hostOrigin: string,
  signal?: AbortSignal,
  timeoutMs = 20_000,
): Promise<MessagePort> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const cleanup = () => {
      window.removeEventListener("message", onMessage);
      signal?.removeEventListener("abort", onAbort);
      if (timer !== null) clearTimeout(timer);
    };
    const finish = (port: MessagePort) => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(port);
    };
    const fail = (error: Error) => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(error);
    };
    const onAbort = () => fail(new Error("waitForIframePort aborted"));
    const onMessage = (event: MessageEvent) => {
      if (event.source !== window.parent) return;
      if (event.origin !== hostOrigin) return;
      if (event.data?.type !== "truapi-init") return;
      const port = event.ports[0];
      if (!port) {
        fail(new Error("truapi-init did not include a MessagePort"));
        return;
      }
      finish(port);
    };

    window.addEventListener("message", onMessage);
    signal?.addEventListener("abort", onAbort, { once: true });
    timer = setTimeout(
      () =>
        fail(
          new Error(
            `Timed out waiting for iframe MessagePort (${timeoutMs}ms)`,
          ),
        ),
      timeoutMs,
    );

    window.parent.postMessage({ type: "truapi-playground-ready" }, hostOrigin);
  });
}

/** Origin used as the `targetOrigin` argument for outbound `postMessage`
 * frames. `document.referrer` is the URL of the parent document that loaded
 * the iframe, so its origin is the host that's expected to receive our
 * frames. We refuse to send if we can't pin a concrete origin (rather than
 * falling back to `"*"`), since the frames carry signed payloads and account
 * ids that must not leak to an unrelated frame parent. */
function resolveHostOrigin(): string | null {
  if (typeof document !== "undefined" && document.referrer) {
    try {
      return new URL(document.referrer).origin;
    } catch {
      // fall through to ancestorOrigins
    }
  }
  const ancestors = window.location.ancestorOrigins;
  if (ancestors && ancestors.length > 0) return ancestors[0];
  return null;
}

function createSandboxProvider(): Provider {
  if (isIframe()) {
    const hostOrigin = resolveHostOrigin();
    if (!hostOrigin) {
      throw new Error(
        "Iframe provider could not resolve the host origin from document.referrer; " +
          "the playground must be embedded by a host that sends a Referer header.",
      );
    }
    const portController = new AbortController();
    const provider = createMessagePortProvider(
      waitForIframePort(hostOrigin, portController.signal),
    );
    const baseDispose = provider.dispose;
    provider.dispose = () => {
      portController.abort();
      baseDispose?.();
    };
    return provider;
  }
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
      return () => {
        _statusListeners.delete(callback);
      };
    }
    setStatus("connecting");
    try {
      const handshake = Promise.resolve(ensureClient().system.handshake()).then(
        (result) => result.isOk(),
      );
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
