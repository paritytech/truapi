import assert from "node:assert/strict";
import test from "node:test";

import {
  HostDevicePermissionRequest,
  HostDevicePermissionResponse,
  HostFeatureSupportedRequest,
  HostFeatureSupportedResponse,
  HostPushNotificationRequest,
  HostPushNotificationResponse,
  RemotePermissionRequest,
  RemotePermissionResponse,
  ThemeVariant,
} from "@parity/truapi";

import {
  createUnavailableCallbacks,
  createWasmRawCallbacks,
} from "../dist/index.js";

// The generated `createWasmRawCallbacks` adapter speaks the symmetric SCALE
// byte boundary: codec-typed requests arrive as `Uint8Array` and are decoded
// for the typed host callback; codec-typed responses are SCALE-encoded back to
// `Uint8Array`. Primitives, strings, byte blobs and the local `AuthState` pass
// through unchanged.

const GENESIS = `0x${"11".repeat(32)}`;

function settle() {
  return new Promise((resolve) => setImmediate(resolve));
}

test("createUnavailableCallbacks rejects storage write paths", async () => {
  const callbacks = createUnavailableCallbacks();

  await assert.rejects(
    () => callbacks.write("key", new Uint8Array([1])),
    /write unavailable/,
  );
  await assert.rejects(() => callbacks.clear("key"), /clear unavailable/);
  assert.equal(await callbacks.read("key"), undefined);
});

test("createWasmRawCallbacks decodes requests and encodes typed responses", async () => {
  const writes = [];
  const clears = [];
  const cancelled = [];
  const raw = createWasmRawCallbacks({
    pushNotification: async (notification) => ({
      id: notification.text.length,
    }),
    cancelNotification: async (id) => {
      cancelled.push(id);
    },
    devicePermission: async (request) => ({ granted: request === "Camera" }),
    remotePermission: async (request) => ({
      granted: request.permission.tag === "ChainSubmit",
    }),
    featureSupported: async (request) => ({
      supported:
        request.tag === "Chain" && request.value.genesisHash === GENESIS,
    }),
    read: async (key) => new TextEncoder().encode(`read:${key}`),
    write: async (key, value) => {
      writes.push([key, [...value]]);
    },
    clear: async (key) => {
      clears.push(key);
    },
  });

  assert.equal(
    HostPushNotificationResponse.dec(
      await raw.pushNotification(
        HostPushNotificationRequest.enc({
          text: "hello",
          deeplink: undefined,
          scheduledAt: undefined,
        }),
      ),
    ).id,
    5,
  );
  assert.equal(
    HostDevicePermissionResponse.dec(
      await raw.devicePermission(HostDevicePermissionRequest.enc("Camera")),
    ).granted,
    true,
  );
  assert.equal(
    RemotePermissionResponse.dec(
      await raw.remotePermission(
        RemotePermissionRequest.enc({ permission: { tag: "ChainSubmit" } }),
      ),
    ).granted,
    true,
  );
  assert.equal(
    HostFeatureSupportedResponse.dec(
      await raw.featureSupported(
        HostFeatureSupportedRequest.enc({
          tag: "Chain",
          value: { genesisHash: GENESIS },
        }),
      ),
    ).supported,
    true,
  );
  assert.deepEqual(
    await raw.read("session"),
    new TextEncoder().encode("read:session"),
  );

  await raw.write("session", new Uint8Array([1, 2, 3]));
  await raw.clear("session");
  await raw.cancelNotification?.(9);

  assert.deepEqual(writes, [["session", [1, 2, 3]]]);
  assert.deepEqual(clears, ["session"]);
  assert.deepEqual(cancelled, [9]);
});

test("createWasmRawCallbacks bridges lifecycle, confirmations, and preimage callbacks", async () => {
  const calls = [];
  async function* sessionTicks() {
    yield { success: true, value: undefined };
    yield { success: true, value: undefined };
  }
  async function* preimages() {
    yield { success: true, value: undefined };
    yield { success: true, value: new Uint8Array([4, 5, 6]) };
  }

  const raw = createWasmRawCallbacks({
    authStateChanged: (state) => {
      calls.push(["authStateChanged", state]);
    },
    readStoredSession: async () => new Uint8Array([1, 2, 3]),
    writeStoredSession: async (value) => {
      calls.push(["writeStoredSession", [...value]]);
    },
    clearStoredSession: async () => {
      calls.push(["clearStoredSession"]);
    },
    subscribeStoredSession: () => sessionTicks(),
    confirmSignPayload: async (payload) => payload[0] === 1,
    confirmSignRaw: async (payload) => payload[0] === 2,
    confirmCreateTransaction: async (payload) => payload[0] === 3,
    confirmAccountAlias: async (payload) => payload[0] === 4,
    confirmResourceAllocation: async (payload) => payload[0] === 5,
    confirmPreimageSubmit: async (size) => {
      calls.push(["confirmPreimageSubmit", size]);
    },
    submitPreimage: async (value) => {
      calls.push(["submitPreimage", [...value]]);
      return new Uint8Array([7, 8, 9]);
    },
    lookupPreimage: (key) => {
      calls.push(["lookupPreimage", [...key]]);
      return preimages();
    },
  });

  const sessionEvents = [];
  const disposeSession = raw.subscribeStoredSession?.(() =>
    sessionEvents.push("tick"),
  );
  const preimageEvents = [];
  const disposePreimages = raw.lookupPreimage(new Uint8Array([9]), (value) =>
    preimageEvents.push(value ? [...value] : null),
  );

  raw.authStateChanged?.({
    tag: "Pairing",
    value: { deeplink: "polkadotapp://example" },
  });
  assert.deepEqual(await raw.readStoredSession?.(), new Uint8Array([1, 2, 3]));
  await raw.writeStoredSession?.(new Uint8Array([3, 2, 1]));
  await raw.clearStoredSession?.();
  assert.equal(await raw.confirmSignPayload?.(new Uint8Array([1])), true);
  assert.equal(await raw.confirmSignRaw?.(new Uint8Array([2])), true);
  assert.equal(await raw.confirmCreateTransaction?.(new Uint8Array([3])), true);
  assert.equal(await raw.confirmAccountAlias?.(new Uint8Array([4])), true);
  assert.equal(
    await raw.confirmResourceAllocation?.(new Uint8Array([5])),
    true,
  );
  await raw.confirmPreimageSubmit(42);
  assert.deepEqual(
    await raw.submitPreimage(new Uint8Array([6])),
    new Uint8Array([7, 8, 9]),
  );

  await settle();
  await settle();

  assert.deepEqual(sessionEvents, ["tick", "tick"]);
  assert.deepEqual(preimageEvents, [null, [4, 5, 6]]);
  assert.deepEqual(calls, [
    ["lookupPreimage", [9]],
    [
      "authStateChanged",
      { tag: "Pairing", value: { deeplink: "polkadotapp://example" } },
    ],
    ["writeStoredSession", [3, 2, 1]],
    ["clearStoredSession"],
    ["confirmPreimageSubmit", 42n],
    ["submitPreimage", [6]],
  ]);

  disposeSession?.();
  disposePreimages?.();
});

test("createWasmRawCallbacks default session-store subscription emits current tick", () => {
  const raw = createWasmRawCallbacks({});
  const ticks = [];
  raw.subscribeStoredSession?.(() => ticks.push("tick"));
  assert.deepEqual(ticks, ["tick"]);
});

test("createWasmRawCallbacks default theme and preimage subscriptions emit current values", () => {
  const raw = createWasmRawCallbacks({});
  const themes = [];
  raw.subscribeTheme?.((theme) => themes.push(ThemeVariant.dec(theme)));
  assert.deepEqual(themes, ["Dark"]);

  const preimages = [];
  raw.lookupPreimage(new Uint8Array([1]), (value) => preimages.push(value));
  assert.deepEqual(preimages, [undefined]);
});

test("createWasmRawCallbacks adapts typed result subscriptions", async () => {
  async function* themes() {
    yield { success: true, value: "Dark" };
    yield { success: true, value: "Light" };
  }

  const raw = createWasmRawCallbacks({
    subscribeTheme: () => themes(),
  });
  const seen = [];
  const dispose = raw.subscribeTheme?.((theme) =>
    seen.push(ThemeVariant.dec(theme)),
  );

  await settle();
  await settle();

  assert.deepEqual(seen, ["Dark", "Light"]);
  dispose?.();
});

test("createWasmRawCallbacks bridges typed chain connections", async () => {
  const sent = [];
  const responses = ['{"jsonrpc":"2.0","id":1,"result":"ok"}'];
  const raw = createWasmRawCallbacks({
    connect: async (genesisHash) => {
      assert.deepEqual([...genesisHash], Array(32).fill(0x11));
      return {
        send(request) {
          sent.push(request);
        },
        async *responses() {
          yield* responses;
        },
      };
    },
  });

  assert.equal(typeof raw.chainConnect, "function");
  const received = [];
  const connection = await raw.chainConnect?.(GENESIS, (json) =>
    received.push(json),
  );
  assert.ok(connection);

  connection.send('{"jsonrpc":"2.0","id":1,"method":"system_health"}');
  await settle();

  assert.deepEqual(sent, ['{"jsonrpc":"2.0","id":1,"method":"system_health"}']);
  assert.deepEqual(received, responses);
  connection.close();
});
