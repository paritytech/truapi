import type { HostCallbacks } from "./generated/host-callbacks.js";

/** `HostCallbacks` with every optional member required, for exhaustive test fixtures. */
export type CompleteHostCallbacks = Required<HostCallbacks>;

/** Default no-op host callbacks with optional per-test overrides. */
export function makeHostCallbacks(
  overrides: Partial<CompleteHostCallbacks> = {},
): CompleteHostCallbacks {
  return {
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
}

/** Resolve after the current microtask/immediate queue, letting pending async work run. */
export function settle(): Promise<void> {
  return new Promise<void>((resolve) => setImmediate(resolve));
}
