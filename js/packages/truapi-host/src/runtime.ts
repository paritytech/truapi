import type { WireProvider } from "@parity/truapi";
import { CoreStorageKey as GeneratedCoreStorageKey } from "./generated/host-callbacks.js";
import type { CoreAdmin, CoreStorageKey } from "./generated/host-callbacks.js";

// The typed capability interfaces below come straight from the
// `truapi-platform` Rust crate via `truapi-codegen --platform-ts-output`.
// They are the host-author-facing surface: each method takes/returns
// typed wrappers (`HostDevicePermissionRequest`, etc.) rather than raw
// SCALE bytes. The web worker pairing-host runtime adapts this typed surface
// into the byte-oriented callback bridge consumed by the WASM core.
export * from "./generated/host-callbacks.js";
export type { JsonRpcConnection as PlatformJsonRpcConnection } from "./generated/host-callbacks.js";

/** Encode a typed core-storage slot for hosts that need an opaque backing key. */
export function encodeCoreStorageKey(key: CoreStorageKey): Uint8Array {
  return GeneratedCoreStorageKey.enc(key);
}

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

export interface ProductRuntimeConfig {
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

export interface TrUApiProductProvider extends WireProvider, CoreAdmin {
  /**
   * Re-tune the wasm core's log level at runtime. Present on runtimes that
   * keep a live channel to the core (e.g. the Web Worker provider); absent on
   * one-shot constructions that only accept `logLevel` up front.
   */
  setLogLevel?(level: LogLevel): void;
}
