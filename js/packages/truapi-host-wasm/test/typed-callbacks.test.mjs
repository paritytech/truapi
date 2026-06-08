import assert from "node:assert/strict";
import test from "node:test";

import {
  VersionedHostDevicePermissionRequest,
  VersionedHostFeatureSupportedRequest,
  VersionedHostPushNotificationRequest,
  VersionedRemotePermissionRequest,
} from "@parity/truapi";

import { createWasmRawCallbacks } from "../dist/index.js";

const GENESIS = `0x${"11".repeat(32)}`;

function settle() {
  return new Promise((resolve) => setImmediate(resolve));
}

test("createWasmRawCallbacks decodes SCALE request callbacks into typed host calls", async () => {
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
    await raw.pushNotification(
      VersionedHostPushNotificationRequest.enc({
        tag: "V1",
        value: { text: "hello", deeplink: undefined, scheduledAt: undefined },
      }),
    ),
    5,
  );
  assert.equal(
    await raw.devicePermission(
      VersionedHostDevicePermissionRequest.enc({
        tag: "V1",
        value: "Camera",
      }),
    ),
    true,
  );
  assert.equal(
    await raw.remotePermission(
      VersionedRemotePermissionRequest.enc({
        tag: "V1",
        value: { permission: { tag: "ChainSubmit" } },
      }),
    ),
    true,
  );
  assert.equal(
    await raw.featureSupported(
      VersionedHostFeatureSupportedRequest.enc({
        tag: "V1",
        value: { tag: "Chain", value: { genesisHash: GENESIS } },
      }),
    ),
    true,
  );
  assert.deepEqual(
    await raw.localStorageRead("session"),
    new TextEncoder().encode("read:session"),
  );

  await raw.localStorageWrite("session", new Uint8Array([1, 2, 3]));
  await raw.localStorageClear("session");
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
    presentPairing: async (deeplink) => {
      calls.push(["presentPairing", deeplink]);
    },
    readSession: async () => new Uint8Array([1, 2, 3]),
    writeSession: async (value) => {
      calls.push(["writeSession", [...value]]);
    },
    clearSession: async () => {
      calls.push(["clearSession"]);
    },
    subscribeSessionStore: () => sessionTicks(),
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
  const disposeSession = raw.subscribeSessionStore?.(() =>
    sessionEvents.push("tick"),
  );
  const preimageEvents = [];
  const disposePreimages = raw.preimageLookupSubscribe(
    new Uint8Array([9]),
    (value) => preimageEvents.push(value ? [...value] : null),
  );

  await raw.presentPairing?.("polkadotapp://example");
  assert.deepEqual(await raw.readSession?.(), new Uint8Array([1, 2, 3]));
  await raw.writeSession?.(new Uint8Array([3, 2, 1]));
  await raw.clearSession?.();
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
    ["presentPairing", "polkadotapp://example"],
    ["writeSession", [3, 2, 1]],
    ["clearSession"],
    ["confirmPreimageSubmit", 42n],
    ["submitPreimage", [6]],
  ]);

  disposeSession?.();
  disposePreimages?.();
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
  const dispose = raw.themeSubscribe?.((theme) => seen.push(theme));

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
