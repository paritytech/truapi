import type {
  FlatHostCallbacks,
  RequiredHostCallbacks,
} from "./generated/host-callbacks.js";

/** `HostCallbacks` with every optional member required, for exhaustive test fixtures. */
export type CompleteHostCallbacks = RequiredHostCallbacks;

type FlatHostCallbackOverrides = Partial<Required<FlatHostCallbacks>>;

/** Default no-op host callbacks with optional per-test overrides. */
export function makeHostCallbacks(
  overrides: FlatHostCallbackOverrides = {},
): CompleteHostCallbacks {
  const flat: Required<FlatHostCallbacks> = {
    navigateTo: async () => {},
    pushNotification: async () => ({ id: 0 }),
    cancelNotification: async () => {},
    devicePermission: async () => ({ granted: false }),
    remotePermission: async () => ({ granted: false }),
    featureSupported: async () => ({ supported: false }),
    readCoreStorage: async () => undefined,
    writeCoreStorage: async () => {},
    clearCoreStorage: async () => {},
    read: async () => undefined,
    write: async () => {},
    clear: async () => {},
    authStateChanged: () => {},
    confirmUserAction: async () => false,
    submitPreimage: async () => new Uint8Array(),
    async *lookupPreimage() {},
    async *subscribeTheme() {},
    connect: async () => ({
      send() {},
      async *responses() {},
      close() {},
    }),
    ...overrides,
  };
  return {
    navigation: { navigateTo: flat.navigateTo },
    notifications: {
      pushNotification: flat.pushNotification,
      cancelNotification: flat.cancelNotification,
    },
    permissions: {
      devicePermission: flat.devicePermission,
      remotePermission: flat.remotePermission,
    },
    features: { featureSupported: flat.featureSupported },
    productStorage: {
      read: flat.read,
      write: flat.write,
      clear: flat.clear,
    },
    coreStorage: {
      readCoreStorage: flat.readCoreStorage,
      writeCoreStorage: flat.writeCoreStorage,
      clearCoreStorage: flat.clearCoreStorage,
    },
    auth: { authStateChanged: flat.authStateChanged },
    userConfirmation: { confirmUserAction: flat.confirmUserAction },
    preimage: {
      submitPreimage: flat.submitPreimage,
      lookupPreimage: flat.lookupPreimage,
    },
    theme: { subscribeTheme: flat.subscribeTheme },
    chain: { connect: flat.connect },
  };
}

/** Resolve after the current microtask/immediate queue, letting pending async work run. */
export function settle(): Promise<void> {
  return new Promise<void>((resolve) => setImmediate(resolve));
}
