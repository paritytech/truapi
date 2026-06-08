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
  const raw = createWasmRawCallbacks({
    pushNotification: async (notification) => ({ id: notification.text.length }),
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

  assert.deepEqual(writes, [["session", [1, 2, 3]]]);
  assert.deepEqual(clears, ["session"]);
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

  assert.deepEqual(sent, [
    '{"jsonrpc":"2.0","id":1,"method":"system_health"}',
  ]);
  assert.deepEqual(received, responses);
  connection.close();
});
