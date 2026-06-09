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
  ChainProvider,
  Features,
  HostCallbacks,
  JsonRpcConnection as PlatformJsonRpcConnection,
  Navigation,
  Notifications,
  Permissions,
  PreimageHost,
  Storage,
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
export interface WasmRawCallbacks {
  navigateTo(url: string): Promise<void>;
  pushNotification(payload: Uint8Array): Promise<number>;
  cancelNotification?(id: number): Promise<void>;
  devicePermission(payload: Uint8Array): Promise<boolean>;
  remotePermission(payload: Uint8Array): Promise<boolean>;
  featureSupported(payload: Uint8Array): Promise<boolean>;
  localStorageRead(key: string): Promise<Uint8Array | null | undefined>;
  localStorageWrite(key: string, value: Uint8Array): Promise<void>;
  localStorageClear(key: string): Promise<void>;
  presentPairing?(deeplink: string): Promise<void>;
  readSession?(): Promise<Uint8Array | null | undefined>;
  writeSession?(value: Uint8Array): Promise<void>;
  clearSession?(): Promise<void>;
  subscribeSessionStore?(sendItem: () => void): (() => void) | void;
  confirmSignPayload?(payload: Uint8Array): Promise<boolean>;
  confirmSignRaw?(payload: Uint8Array): Promise<boolean>;
  confirmCreateTransaction?(payload: Uint8Array): Promise<boolean>;
  confirmAccountAlias?(payload: Uint8Array): Promise<boolean>;
  confirmResourceAllocation?(payload: Uint8Array): Promise<boolean>;
  confirmPreimageSubmit?(size: number): Promise<void>;
  submitPreimage?(value: Uint8Array): Promise<Uint8Array>;
  themeSubscribe?(
    sendItem: (theme: "Light" | "Dark" | 0 | 1 | Uint8Array) => void,
  ): (() => void) | void;
  preimageLookupSubscribe(
    key: Uint8Array,
    sendItem: (value: Uint8Array | null | undefined) => void,
  ): (() => void) | void;
  /** Optional. When omitted, the WASM bridge reports chain calls as
   * "unavailable". Hosts that own chain access (e.g. dotli's
   * smoldot/RPC toggle) supply it. */
  chainConnect?: ChainConnect;
  emitFrame(frame: Uint8Array): void;
  dispose?(): void;
}

/**
 * Stubs every required callback so a host can spread them over its own
 * implementation and override only what it supports. Unavailable methods
 * reject with a descriptive error; unavailable subscriptions emit a current
 * default item where the platform contract requires one.
 */
export function createUnavailableCallbacks(): Omit<
  WasmRawCallbacks,
  "emitFrame" | "dispose" | "chainConnect"
> {
  const unavailable = (method: string) => async (): Promise<never> => {
    throw new Error(`${method} unavailable on this host`);
  };
  const emitCurrentTick = (sendItem: () => void): void => {
    sendItem();
  };
  const emitCurrentTheme = (
    sendItem: (theme: "Light" | "Dark" | 0 | 1 | Uint8Array) => void,
  ): void => {
    sendItem("Dark");
  };
  const emitCurrentPreimageMiss = (
    _key: Uint8Array,
    sendItem: (value: Uint8Array | null | undefined) => void,
  ): void => {
    sendItem(undefined);
  };
  return {
    navigateTo: unavailable("navigateTo"),
    pushNotification: async () => 0,
    devicePermission: async () => false,
    remotePermission: async () => false,
    featureSupported: unavailable("featureSupported"),
    localStorageRead: async () => undefined,
    localStorageWrite: async () => {},
    localStorageClear: async () => {},
    confirmPreimageSubmit: unavailable("confirmPreimageSubmit"),
    submitPreimage: unavailable("submitPreimage"),
    subscribeSessionStore: emitCurrentTick,
    themeSubscribe: emitCurrentTheme,
    preimageLookupSubscribe: emitCurrentPreimageMiss,
  };
}

/**
 * Shape exposed by the wasm-pack output's `WasmTrUApiCore`. Kept local
 * so the package does not have a hard dependency on the generated `.d.ts`
 * file path.
 */
export interface WasmCoreLike {
  receiveFromProduct(frame: Uint8Array): Promise<void>;
  disconnect?(): Promise<void>;
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
  partial: Omit<WasmRawCallbacks, "emitFrame">,
): TrUApiHostWasmProvider {
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  let disposed = false;

  const raw: WasmRawCallbacks = {
    ...partial,
    emitFrame(frame: Uint8Array) {
      if (disposed) return;
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
      if (disposed) return;
      void core.receiveFromProduct(bytes).catch((err: unknown) => {
        const error = err instanceof Error ? err : new Error(String(err));
        for (const listener of [...closeListeners]) listener(error);
      });
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
    async disconnect() {
      if (disposed) return;
      if (!core.disconnect) {
        throw new Error("disconnect unavailable on this WASM core");
      }
      await core.disconnect();
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
      listeners.clear();
      closeListeners.clear();
      partial.dispose?.();
    },
  };
}
