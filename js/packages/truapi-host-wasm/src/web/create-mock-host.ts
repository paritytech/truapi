// A deterministic, in-memory mock host. `createMockHost` returns a complete
// `HostCallbacks` set (the JS sibling of `truapi-platform`'s `MockPlatform`)
// plus recordings for assertions. Hand `host.callbacks` to
// `createWebWorkerProvider` (or `createIframeHost`) to run the real
// truapi-server WASM core against a mocked OS seam: storage is in-memory,
// permissions answer from a fixed policy, navigation/notifications are
// recorded, and the chain connection is silent (or replays canned frames).
//
// Signing and login require a paired wallet answering over the statement-store
// channel; the default silent chain records outbound requests and never
// answers, so those flows park. Everything else (storage, permissions,
// features, theme, navigation, notifications, preimage) works without a wallet.

import { ok } from "neverthrow";

import type {
  GenericError,
  HostPushNotificationRequest,
  Result,
  ThemeVariant,
} from "@parity/truapi";

import type {
  AuthState,
  CoreStorageKey,
  HostCallbacks,
  JsonRpcConnection,
} from "../generated/host-callbacks.js";
import type { HostCoreRuntimeConfig } from "../runtime.js";

/** How the mock answers a permission prompt for one capability. */
export type PermissionPolicy = "allow-all" | "deny-all";

/** Behavior knobs for {@link createMockHost}. */
export interface MockHostConfig {
  /** Answer for `devicePermission`. Default `"allow-all"`. */
  devicePermissions?: PermissionPolicy;
  /** Answer for `remotePermission`. Default `"allow-all"`. */
  remotePermissions?: PermissionPolicy;
  /** Whether `featureSupported` reports support. Default `true`. */
  featureSupported?: boolean;
  /** Theme emitted by `subscribeTheme`. Default `"Dark"`. */
  theme?: ThemeVariant;
  /** Whether `confirmUserAction` confirms reviewed actions. Default `true`. */
  confirmUserActions?: boolean;
  /**
   * JSON-RPC response frames the chain connection replays, in order. Empty
   * (the default) means a silent connection: it records outbound requests and
   * never answers, so chain-dependent flows park.
   */
  chainResponses?: string[];
  /**
   * When `true`, the chain response stream ends immediately instead of parking,
   * so disconnect/timeout paths can be asserted (fail-fast). Ignored when
   * `chainResponses` is non-empty.
   */
  chainClosed?: boolean;
}

/** A mock host: the callbacks to wire into a provider, plus assertion oracles. */
export interface MockHost {
  /** Hand this to `createWebWorkerProvider` / `createIframeHost`. */
  callbacks: HostCallbacks;
  /** URLs the core asked the host to open, in order. */
  navigations(): string[];
  /** Notifications the core asked the host to show, in order. */
  pushedNotifications(): HostPushNotificationRequest[];
  /** Raw JSON-RPC the core sent over the chain connection, in order. */
  sentRpc(): string[];
  /** Auth-state transitions the core emitted, in order. */
  authStates(): AuthState[];
  /** Confirmation kinds the core requested (review `tag`s), in order. */
  confirmations(): string[];
  /** Notification ids the core asked the host to cancel, in order. */
  cancelledNotifications(): number[];
}

/** Deterministic 8-byte key for a preimage value (FNV-1a), so `submitPreimage`
 *  then `lookupPreimage` round-trips without using the full value as its key. */
function preimageKey(value: Uint8Array): Uint8Array {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  for (const byte of value) {
    hash = ((hash ^ BigInt(byte)) * prime) & mask;
  }
  const key = new Uint8Array(8);
  for (let i = 0; i < 8; i++) {
    key[i] = Number((hash >> BigInt(8 * i)) & 0xffn);
  }
  return key;
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Build an in-memory mock host. The returned `callbacks` implement every
 * `HostCallbacks` capability; the accessor methods expose what the core did.
 */
export function createMockHost(config: MockHostConfig = {}): MockHost {
  const {
    devicePermissions = "allow-all",
    remotePermissions = "allow-all",
    featureSupported = true,
    theme = "Dark",
    confirmUserActions = true,
    chainResponses = [],
    chainClosed = false,
  } = config;

  const storage = new Map<string, Uint8Array>();
  const preimages = new Map<string, Uint8Array>();
  const navigations: string[] = [];
  const pushedNotifications: HostPushNotificationRequest[] = [];
  const sentRpc: string[] = [];
  const authStates: AuthState[] = [];
  const confirmations: string[] = [];
  const cancelledNotifications: number[] = [];
  let nextNotificationId = 0;

  // Product keys are namespaced from core slots so neither can shadow the other.
  // This in-JS key scheme is internal and independent from the Rust MockPlatform's
  // (state never crosses the boundary), so the two need not match byte-for-byte.
  const productKey = (key: string): string => `product:${key}`;
  const coreKey = (key: CoreStorageKey): string =>
    key.tag === "PermissionAuthorization"
      ? `core:permission:${key.value.productId}:${JSON.stringify(key.value.request)}`
      : `core:${key.tag}`;
  const granted = (policy: PermissionPolicy): boolean => policy === "allow-all";

  // `Required<HostCallbacks>` (not bare `HostCallbacks`): every optional callback
  // must be present, so a capability added to the generated surface fails `tsc`
  // here until the mock covers it. This is the load-bearing coverage guarantee.
  const callbacks: Required<HostCallbacks> = {
    // ProductStorage
    async read(key) {
      return storage.get(productKey(key));
    },
    async write(key, value) {
      storage.set(productKey(key), value);
    },
    async clear(key) {
      storage.delete(productKey(key));
    },

    // CoreStorage
    async readCoreStorage(key) {
      return storage.get(coreKey(key));
    },
    async writeCoreStorage(key, value) {
      storage.set(coreKey(key), value);
    },
    async clearCoreStorage(key) {
      storage.delete(coreKey(key));
    },

    // Navigation
    async navigateTo(url) {
      navigations.push(url);
    },

    // Notifications
    async pushNotification(notification) {
      pushedNotifications.push(notification);
      return { id: nextNotificationId++ };
    },
    async cancelNotification(id) {
      cancelledNotifications.push(id);
    },

    // Permissions
    async devicePermission() {
      return { granted: granted(devicePermissions) };
    },
    async remotePermission() {
      return { granted: granted(remotePermissions) };
    },

    // Features
    async featureSupported() {
      return { supported: featureSupported };
    },

    // ChainProvider
    async connect(): Promise<JsonRpcConnection> {
      return {
        send(request) {
          sentRpc.push(request);
        },
        async *responses(): AsyncGenerator<string> {
          for (const frame of chainResponses) {
            yield frame;
          }
          if (chainResponses.length === 0 && !chainClosed) {
            // Silent: never yields, so chain-dependent flows park. `chainClosed`
            // instead ends the stream here for fail-fast disconnect tests.
            await new Promise<never>(() => {});
          }
        },
        // The mock holds no real transport, so releasing the lease is a no-op.
        // Note: a Silent connection whose `responses()` stream is already parked
        // stays parked after close() — tests that need the stream to terminate use
        // `chainClosed` (or scripted frames), not close().
        close() {},
      };
    },

    // AuthPresenter
    authStateChanged(state) {
      authStates.push(state);
    },

    // UserConfirmation
    async confirmUserAction(review) {
      confirmations.push(review.tag);
      return confirmUserActions;
    },

    // ThemeHost
    async *subscribeTheme(): AsyncGenerator<
      Result<ThemeVariant, GenericError>
    > {
      yield ok(theme);
      // A live subscription never ends: emit the current theme, then stay open.
      await new Promise<never>(() => {});
    },

    // PreimageHost
    async submitPreimage(value) {
      const key = preimageKey(value);
      preimages.set(hex(key), value);
      return key;
    },
    async *lookupPreimage(
      key,
    ): AsyncGenerator<Result<Uint8Array | undefined, GenericError>> {
      yield ok(preimages.get(hex(key)));
      // Stay open for future updates (none, in the mock).
      await new Promise<never>(() => {});
    },
  };

  return {
    callbacks,
    navigations: () => [...navigations],
    pushedNotifications: () => [...pushedNotifications],
    sentRpc: () => [...sentRpc],
    authStates: () => [...authStates],
    confirmations: () => [...confirmations],
    cancelledNotifications: () => [...cancelledNotifications],
  };
}

/**
 * A default {@link HostCoreRuntimeConfig} for a mock host. Override any field;
 * the genesis hash and product id are placeholders suitable for tests.
 */
export function mockRuntimeConfig(
  overrides: Partial<HostCoreRuntimeConfig> = {},
): HostCoreRuntimeConfig {
  return {
    productId: "mock.product",
    host: {
      name: "Mock Host",
      icon: "https://example.invalid/mock.png",
      version: "0.0.0",
    },
    platform: {
      type: "node",
      version: "0",
    },
    people: {
      genesisHash:
        "0x0000000000000000000000000000000000000000000000000000000000000000",
    },
    pairing: {
      deeplinkScheme: "polkadotapp",
    },
    ...overrides,
  };
}
