import type { WireProvider } from "@parity/truapi";

// The typed capability interfaces below come straight from the
// `truapi-platform` Rust crate via `truapi-codegen --platform-ts-output`.
// They are the host-author-facing surface: each method takes/returns
// typed wrappers (`HostDevicePermissionRequest`, etc.) rather than raw
// SCALE bytes. `createWebWorkerProvider` adapts this typed surface into
// the byte-oriented callback bridge consumed by the WASM core.
export type {
  AuthState,
  ChainProvider,
  CoreStorage,
  CoreStorageKey,
  Features,
  HostCallbacks,
  JsonRpcConnection as PlatformJsonRpcConnection,
  Navigation,
  Notifications,
  Permissions,
  PreimageHost,
  ProductStorage,
  SessionUiInfo,
  ThemeHost,
} from "./generated/host-callbacks.js";

/**
 * Async-or-sync return. Synchronous hosts (e.g. the dotli main-thread
 * shell hitting localStorage) can return a plain value; the WASM bridge
 * awaits every return so an `async` impl also works.
 */
export type Awaitable<T> = T | Promise<T>;

/**
 * Open a JSON-RPC connection for `genesisHash`. The wasm bridge passes
 * `onResponse` so the host can push JSON-RPC replies back asynchronously.
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
 * Verbosity threshold for the wasm core's `tracing` output. The Rust core
 * parses the string; known values are `off`, `error`, `warn`, `info`, `debug`,
 * and `trace`.
 */
export type LogLevel = string;

export interface HostCoreRuntimeConfig {
  productId: string;
  host: {
    name: string;
    icon?: string;
    version?: string;
  };
  platform?: {
    type?: string;
    version?: string;
  };
  people: {
    genesisHash: string | Uint8Array;
  };
  pairing: {
    deeplinkScheme: string;
  };
}

export interface TrUApiHostCoreProvider extends WireProvider {
  /**
   * Core-owned logout/disconnect. This best-effort notifies the SSO peer,
   * clears the in-memory session, clears CoreStorage auth state, and broadcasts
   * Disconnected from the Rust core.
   */
  disconnectSession(): Promise<void>;

  /**
   * Cancel any in-flight `requestLogin` pairing (e.g. the user closed the
   * pairing UI). The core emits a `Disconnected` auth state and resolves
   * the pending login as `Rejected`. A no-op when no login is in progress.
   */
  cancelPairing(): void;

  /**
   * Notify the core that the host-global auth session may have changed. The
   * core will re-read the stored blob and emit any resulting auth/session
   * state updates.
   */
  notifySessionStoreChanged(): void;

  /**
   * Re-tune the wasm core's log level at runtime. Present on runtimes that
   * keep a live channel to the core (e.g. the Web Worker provider); absent on
   * one-shot constructions that only accept `logLevel` up front.
   */
  setLogLevel?(level: LogLevel): void;
}
