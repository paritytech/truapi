import type { Provider } from "@parity/truapi";

/**
 * Minimal subset of Electron's `MessagePortMain` interface used by this
 * package. Kept local so the package does not have a hard `electron`
 * dependency (the host code passes the port in at runtime).
 */
export interface ElectronMessagePortMain {
  postMessage(message: unknown, transfer?: unknown[]): void;
  on(event: "message", handler: (event: { data: unknown }) => void): this;
  on(event: "close", handler: () => void): this;
  off(event: "message", handler: (event: { data: unknown }) => void): this;
  off(event: "close", handler: () => void): this;
  start(): void;
  close(): void;
}

/**
 * Options for `createElectronProvider`.
 */
export interface CreateElectronProviderOptions {
  /** One end of an Electron `MessageChannelMain`. The other end must be
   * transferred to the renderer through the preload script. */
  port: ElectronMessagePortMain;
}

/**
 * Wrap an Electron `MessagePortMain` as a TrUAPI `Provider`. The
 * provider exchanges SCALE-encoded `Uint8Array` frames with the renderer.
 * The provider's `dispose` closes the port.
 *
 * Hosts typically pair this with `@parity/host-shared`'s
 * `createNodeWasmProvider` (for the WASM core) and `createHostServer`
 * from `@parity/truapi-host` (for the dispatcher) to assemble a full
 * Electron host.
 */
export function createElectronProvider(
  options: CreateElectronProviderOptions,
): Provider {
  const { port } = options;
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  let disposed = false;

  const onMessage = (event: { data: unknown }): void => {
    if (disposed) return;
    const data = event.data;
    if (!(data instanceof Uint8Array)) return;
    for (const listener of [...listeners]) listener(data);
  };

  const onClose = (): void => {
    const error = new Error("electron message port closed");
    for (const listener of [...closeListeners]) listener(error);
  };

  port.on("message", onMessage);
  port.on("close", onClose);
  port.start();

  return {
    postMessage(bytes: Uint8Array): void {
      if (disposed) return;
      port.postMessage(bytes);
    },
    subscribe(callback) {
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },
    subscribeClose(callback) {
      closeListeners.add(callback);
      return () => {
        closeListeners.delete(callback);
      };
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      try {
        port.off("message", onMessage);
        port.off("close", onClose);
        port.close();
      } catch {
        // already closed
      }
      listeners.clear();
      closeListeners.clear();
    },
  };
}
