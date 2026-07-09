import type { RequiredHostCallbacks } from "./generated/host-callbacks.js";

/** `HostCallbacks` with every optional member required, for exhaustive test fixtures. */
export type CompleteHostCallbacks = RequiredHostCallbacks;

type HostCallbackOverrides = {
  [K in keyof RequiredHostCallbacks]?: Partial<RequiredHostCallbacks[K]>;
};

/** Default no-op host callbacks with optional per-test overrides. */
export function makeHostCallbacks(
  overrides: HostCallbackOverrides = {},
): CompleteHostCallbacks {
  const defaults: CompleteHostCallbacks = {
    navigation: { navigateTo: async () => {} },
    notifications: {
      pushNotification: async () => ({ id: 0 }),
      cancelNotification: async () => {},
    },
    permissions: {
      devicePermission: async () => ({ granted: false }),
      remotePermission: async () => ({ granted: false }),
    },
    features: { featureSupported: async () => ({ supported: false }) },
    productStorage: {
      read: async () => undefined,
      write: async () => {},
      clear: async () => {},
    },
    coreStorage: {
      readCoreStorage: async () => undefined,
      writeCoreStorage: async () => {},
      clearCoreStorage: async () => {},
    },
    auth: { authStateChanged: () => {} },
    userConfirmation: { confirmUserAction: async () => false },
    preimage: {
      submitPreimage: async () => new Uint8Array(),
      async *lookupPreimage() {},
    },
    theme: { async *subscribeTheme() {} },
    chain: {
      connect: async () => ({
        send() {},
        async *responses() {},
        close() {},
      }),
    },
  };

  return {
    navigation: { ...defaults.navigation, ...overrides.navigation },
    notifications: {
      ...defaults.notifications,
      ...overrides.notifications,
    },
    permissions: {
      ...defaults.permissions,
      ...overrides.permissions,
    },
    features: { ...defaults.features, ...overrides.features },
    productStorage: {
      ...defaults.productStorage,
      ...overrides.productStorage,
    },
    coreStorage: {
      ...defaults.coreStorage,
      ...overrides.coreStorage,
    },
    auth: { ...defaults.auth, ...overrides.auth },
    userConfirmation: {
      ...defaults.userConfirmation,
      ...overrides.userConfirmation,
    },
    preimage: { ...defaults.preimage, ...overrides.preimage },
    theme: { ...defaults.theme, ...overrides.theme },
    chain: { ...defaults.chain, ...overrides.chain },
  };
}

/** Resolve after the current microtask/immediate queue, letting pending async work run. */
export function settle(): Promise<void> {
  return new Promise<void>((resolve) => setImmediate(resolve));
}
