import type { Provider } from "@parity/truapi";

// The typed capability interfaces below come straight from the
// `truapi-platform` Rust crate via `truapi-codegen --platform-ts-output`.
// They are the host-author-facing surface: each method takes/returns
// typed wrappers (`HostDevicePermissionRequest`, etc.) rather than raw
// SCALE bytes. The `WasmRawCallbacks` interface declared further down
// is the byte-oriented wire surface the WASM core invokes; use
// `createWasmRawCallbacks` to adapt this typed surface into the raw
// callback surface consumed by `createWasmProvider`.
export type {
  AuthState,
  ChainProvider,
  Features,
  HostCallbacks,
  JsonRpcConnection as PlatformJsonRpcConnection,
  Navigation,
  Notifications,
  Permissions,
  PreimageHost,
  SessionUiInfo,
  HostStorage,
  ThemeHost,
} from "./generated/host-callbacks.js";
import type { HostCallbacks } from "./generated/host-callbacks.js";
import type { RawCallbacks } from "./generated/host-callbacks-adapter.js";
import { createWasmRawCallbacks } from "./generated/host-callbacks-adapter.js";

/**
 * Async-or-sync return. Synchronous hosts (e.g. the dotli main-thread
 * shell hitting localStorage) can return a plain value; the WASM bridge
 * awaits every return so an `async` impl also works.
 */
export type Awaitable<T> = T | Promise<T>;

/**
 * Open a JSON-RPC connection for `genesisHash`. The wasm bridge passes
 * `onResponse` so the host can push smoldot replies back asynchronously.
 * Returning `null` (or throwing) tells the core no provider is available.
 */
export type ChainConnect = (
  genesisHash: string,
  onResponse: (json: string) => void,
) => Awaitable<ChainConnection | null>;

/**
 * Per-connection handle returned by `chainConnect`. `send` forwards a
 * SCALE-encoded JSON-RPC request; `close` tears the connection down.
 */
export interface ChainConnection {
  send(request: string): void;
  close(): void;
}

/**
 * Verbosity threshold for the wasm core's `tracing` output. `off` silences
 * it; the rest map to the matching browser `console` method (`debug`/`trace`
 * land on `console.debug`, hidden in Chrome unless the console level dropdown
 * includes "Verbose").
 */
export type LogLevel = "off" | "error" | "warn" | "info" | "debug" | "trace";

export interface WasmRuntimeConfig {
  productLabel: string;
  productId: string;
  siteId: string;
  hostName: string;
  hostIcon?: string;
  hostVersion?: string;
  platformType?: string;
  platformVersion?: string;
  peopleChainGenesisHash: string | Uint8Array;
  pairingDeeplinkScheme:
    | "polkadotapp"
    | "polkadotApp"
    | "PolkadotApp"
    | "polkadotappdev"
    | "polkadotAppDev"
    | "PolkadotAppDev";
}

/**
 * Raw byte-oriented callbacks the WASM core invokes. Names match the
 * camelCase property keys the Rust `JsBridge::from_js` extracts. Request
 * callbacks return `Promise<Uint8Array>` (or `Promise<bool>` for the
 * permission prompts); subscription callbacks accept a `sendItem` sink
 * and return an optional `dispose` function.
 *
 * This interface is the SCALE-byte-level wire surface between the WASM
 * core and JS; the typed `HostCallbacks` interface above is the
 * host-author surface. They overlap on the capability methods covered by
 * `truapi-platform`; account, signing, and statement-store methods are owned
 * by the Rust core and do not cross this callback boundary.
 */
export type WasmRawCallbacks = RawCallbacks & {
  emitFrame(frame: Uint8Array): void;
  dispose?(): void;
};

/**
 * Shape exposed by the wasm-pack output's `WasmTrUApiCore`. Kept local
 * so the package does not have a hard dependency on the generated `.d.ts`
 * file path.
 */
export interface WasmCoreLike {
  receiveFromProduct(frame: Uint8Array): Promise<void>;
  disconnect?(): Promise<void>;
  cancelLogin?(): void;
  dispose(): void;
  free(): void;
}

export interface TrUApiHostWasmProvider extends Provider {
  /**
   * Core-owned logout/disconnect. This best-effort notifies the SSO peer,
   * clears the in-memory session, clears SessionStore, and broadcasts
   * Disconnected from the Rust core.
   */
  disconnect(): Promise<void>;

  /**
   * Cancel any in-flight `requestLogin` pairing (e.g. the user closed the
   * pairing UI). The core emits a `Disconnected` auth state and resolves
   * the pending login as `Rejected`. A no-op when no login is in progress.
   */
  cancelLogin(): void;

  /**
   * Re-tune the wasm core's log level at runtime. Present on runtimes that
   * keep a live channel to the core (e.g. the Web Worker provider); absent on
   * one-shot constructions that only accept `logLevel` up front.
   */
  setLogLevel?(level: LogLevel): void;
}

/**
 * Wraps a WASM core in a `Provider`, the byte transport abstraction
 * exposed by `@parity/truapi`. The provider can be handed to
 * `createHostServer` from `@parity/truapi-host` so the dispatcher dispatches
 * inbound frames into the WASM core and forwards core-emitted frames back
 * to the listener registered through `provider.subscribe`.
 */
export function createWasmProvider(
  createCore: (rawCallbacks: WasmRawCallbacks) => WasmCoreLike,
  host: Partial<HostCallbacks>,
): TrUApiHostWasmProvider {
  const partial = createWasmRawCallbacks(host);
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  let disposed = false;
  let closedError: Error | null = null;

  // Terminal close-once transition, matching `createBaseProvider` in
  // @parity/truapi: notify close listeners exactly once, then drop all
  // listeners so the provider stops delivering.
  const close = (error: Error): void => {
    if (closedError) return;
    closedError = error;
    for (const listener of [...closeListeners]) listener(error);
    listeners.clear();
    closeListeners.clear();
  };

  const raw: WasmRawCallbacks = {
    ...partial,
    emitFrame(frame: Uint8Array) {
      if (disposed || closedError) return;
      // Copy out of the WASM-owned buffer so retained references stay
      // valid once the core reuses the underlying memory.
      const copy = new Uint8Array(frame.length);
      copy.set(frame);
      for (const listener of [...listeners]) listener(copy);
    },
  };

  const core = createCore(raw);

  return {
    postMessage(bytes: Uint8Array): void {
      if (disposed || closedError) return;
      void core.receiveFromProduct(bytes).catch((err: unknown) => {
        close(err instanceof Error ? err : new Error(String(err)));
      });
    },
    subscribe(callback) {
      if (closedError) return () => {};
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },
    subscribeClose(callback) {
      if (closedError) {
        callback(closedError);
        return () => {};
      }
      closeListeners.add(callback);
      return () => {
        closeListeners.delete(callback);
      };
    },
    async disconnect() {
      if (disposed || closedError) return;
      if (!core.disconnect) {
        throw new Error("disconnect unavailable on this WASM core");
      }
      await core.disconnect();
    },
    cancelLogin() {
      if (disposed || closedError) return;
      core.cancelLogin?.();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      try {
        core.dispose();
      } catch {
        // host dispose threw, swallow during teardown
      }
      try {
        core.free();
      } catch {
        // already freed
      }
      close(new Error("wasm provider disposed"));
    },
  };
}
