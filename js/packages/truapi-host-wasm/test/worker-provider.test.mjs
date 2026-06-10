import assert from "node:assert/strict";
import test from "node:test";

import {
  HostPushNotificationRequest,
} from "../../truapi/dist/index.js";
import {
  createWasmRawCallbacks,
} from "../dist/index.js";
import { createWebWorkerProvider } from "../dist/web/index.js";

class FakeWorker {
  constructor() {
    this.listeners = new Map();
    this.messages = [];
    this.terminated = false;
  }

  addEventListener(name, fn) {
    const listeners = this.listeners.get(name) ?? new Set();
    listeners.add(fn);
    this.listeners.set(name, listeners);
  }

  removeEventListener(name, fn) {
    this.listeners.get(name)?.delete(fn);
  }

  postMessage(message) {
    this.messages.push(message);
  }

  terminate() {
    this.terminated = true;
  }

  emit(message) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }

  emitError(message) {
    for (const listener of this.listeners.get("error") ?? []) {
      listener({ message });
    }
  }

  emitMessageError() {
    for (const listener of this.listeners.get("messageerror") ?? []) {
      listener({ data: null });
    }
  }
}

function makeCallbacks(overrides = {}) {
  return {
    navigateTo: async () => {},
    pushNotification: async () => 0,
    devicePermission: async () => false,
    remotePermission: async () => false,
    featureSupported: async () => false,
    localStorageRead: async () => undefined,
    localStorageWrite: async () => {},
    localStorageClear: async () => {},
    ...overrides,
  };
}

function runtimeConfig(overrides = {}) {
  return {
    productLabel: "dotli",
    productId: "dotli.dot",
    siteId: "dot.li",
    hostName: "Polkadot Web",
    hostIcon: "https://dot.li/dotli.png",
    hostVersion: "0.5.0",
    platformType: "node",
    platformVersion: process.versions.node,
    peopleChainGenesisHash:
      "0xa22a2424d2cbf561eaecf7da8b1b548fa9d1939f60265e942b1049616a012f71",
    pairingDeeplinkScheme: "polkadotapp",
    ...overrides,
  };
}

async function settle() {
  await new Promise((resolve) => setImmediate(resolve));
}

test("createWebWorkerProvider advertises only supplied optional hooks", async () => {
  const worker = new FakeWorker();
  const config = runtimeConfig();
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      clearSession: async () => {},
      readSession: async () => new Uint8Array([1]),
      sessionUiChanged: () => {},
      subscribeSessionStore: () => () => {},
      preimageLookupSubscribe: () => () => {},
      chainConnect: () => ({ send: () => {}, close: () => {} }),
    }),
    {
      logLevel: "debug",
      runtimeConfig: config,
    },
  );

  worker.emit({ kind: "loaded" });
  assert.equal(worker.messages.length, 1);
  assert.deepEqual(worker.messages[0], {
    kind: "init",
    logLevel: "debug",
    runtimeConfig: config,
    optionalCallbacks: ["readSession", "clearSession", "sessionUiChanged"],
    optionalSubscriptions: ["sessionStoreSubscribe", "preimageLookupSubscribe"],
    chainConnect: true,
  });

  worker.emit({ kind: "ready" });
  const provider = await providerPromise;
  assert.equal(typeof provider.disconnect, "function");

  provider.dispose();
});

test("worker provider resolves disconnect responses", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(worker, makeCallbacks(), {
    runtimeConfig: runtimeConfig(),
  });
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  const disconnect = provider.disconnect();
  const msg = worker.messages.at(-1);
  assert.equal(msg.kind, "disconnect");
  assert.equal(typeof msg.requestId, "number");

  worker.emit({
    kind: "disconnectResponse",
    requestId: msg.requestId,
    ok: true,
  });
  await disconnect;

  provider.dispose();
});

test("worker provider dispatches optional callback requests to host hooks", async () => {
  const worker = new FakeWorker();
  let clears = 0;
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      clearSession: async () => {
        clears += 1;
      },
    }),
    {
      runtimeConfig: runtimeConfig(),
    },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  worker.emit({
    kind: "callbackRequest",
    requestId: 7,
    name: "clearSession",
    args: [],
  });
  await settle();

  assert.equal(clears, 1);
  assert.deepEqual(worker.messages.at(-1), {
    kind: "callbackResponse",
    requestId: 7,
    ok: true,
    value: undefined,
  });

  provider.dispose();
});

test("worker provider forwards sessionUiChanged callback requests", async () => {
  const worker = new FakeWorker();
  const infos = [];
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      sessionUiChanged: (info) => {
        infos.push(info);
      },
    }),
    {
      runtimeConfig: runtimeConfig(),
    },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  worker.emit({
    kind: "callbackRequest",
    requestId: 3,
    name: "sessionUiChanged",
    args: [
      {
        connected: true,
        publicKey: new Uint8Array([1, 2]),
        liteUsername: "alice",
      },
    ],
  });
  await settle();

  assert.deepEqual(infos, [
    { connected: true, publicKey: new Uint8Array([1, 2]), liteUsername: "alice" },
  ]);
  assert.deepEqual(worker.messages.at(-1), {
    kind: "callbackResponse",
    requestId: 3,
    ok: true,
    value: undefined,
  });

  provider.dispose();
});

test("worker fault terminates the worker and runs the full teardown", async () => {
  const worker = new FakeWorker();
  let subscriptionDisposes = 0;
  let chainCloses = 0;
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      subscribeSessionStore: () => () => {
        subscriptionDisposes += 1;
      },
      chainConnect: () => ({
        send: () => {},
        close: () => {
          chainCloses += 1;
        },
      }),
    }),
    { runtimeConfig: runtimeConfig() },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  worker.emit({
    kind: "subscriptionStart",
    subId: 1,
    name: "sessionStoreSubscribe",
    payload: null,
  });
  worker.emit({ kind: "chainConnectStart", connId: 1, genesisHash: "0xab" });
  await settle();

  const closes = [];
  provider.subscribeClose((error) => closes.push(error));

  worker.emitError("boom");

  assert.equal(worker.terminated, true);
  assert.equal(subscriptionDisposes, 1);
  assert.equal(chainCloses, 1);
  assert.equal(closes.length, 1);
  assert.match(closes[0].message, /boom/);

  // The fault teardown is terminal; a second fault is a no-op.
  worker.emitError("again");
  assert.equal(closes.length, 1);
});

test("worker provider routes payload-carrying subscriptions by name", async () => {
  const worker = new FakeWorker();
  const keys = [];
  let push;
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      preimageLookupSubscribe: (key, sendItem) => {
        keys.push(key);
        push = sendItem;
        return () => {};
      },
    }),
    { runtimeConfig: runtimeConfig() },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  worker.emit({
    kind: "subscriptionStart",
    subId: 4,
    name: "preimageLookupSubscribe",
    payload: new Uint8Array([9, 9]),
  });

  assert.deepEqual(keys, [new Uint8Array([9, 9])]);
  push(new Uint8Array([1]));
  assert.deepEqual(worker.messages.at(-1), {
    kind: "subscriptionItem",
    subId: 4,
    value: new Uint8Array([1]),
  });

  provider.dispose();
});

test("unknown subscription names never fall through to another callback", async () => {
  const worker = new FakeWorker();
  let preimageStarts = 0;
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      preimageLookupSubscribe: () => {
        preimageStarts += 1;
        return () => {};
      },
    }),
    { runtimeConfig: runtimeConfig() },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  worker.emit({
    kind: "subscriptionStart",
    subId: 5,
    name: "someFutureSubscribe",
    payload: new Uint8Array([1, 2, 3]),
  });

  assert.equal(preimageStarts, 0);
  assert.equal(
    worker.messages.some((m) => m.kind === "subscriptionItem"),
    false,
  );

  provider.dispose();
});

test("payload-carrying subscription without payload is not dispatched", async () => {
  const worker = new FakeWorker();
  let preimageStarts = 0;
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      preimageLookupSubscribe: () => {
        preimageStarts += 1;
        return () => {};
      },
    }),
    { runtimeConfig: runtimeConfig() },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  worker.emit({
    kind: "subscriptionStart",
    subId: 6,
    name: "preimageLookupSubscribe",
    payload: null,
  });

  assert.equal(preimageStarts, 0);

  provider.dispose();
});

test("createWebWorkerProvider rejects when init times out", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(worker, makeCallbacks(), {
    runtimeConfig: runtimeConfig(),
    initTimeoutMs: 20,
  });
  worker.emit({ kind: "loaded" });
  await assert.rejects(providerPromise, /worker init timed out after 20ms/);
  assert.equal(worker.terminated, true);
});

test("createWebWorkerProvider rejects on messageerror during init", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(worker, makeCallbacks(), {
    runtimeConfig: runtimeConfig(),
  });
  worker.emitMessageError();
  await assert.rejects(providerPromise, /could not be deserialized/);
  assert.equal(worker.terminated, true);
});

test("typed callbacks decode raw v01 push notification payloads", async () => {
  let notification;
  const callbacks = createWasmRawCallbacks({
    pushNotification: async (request) => {
      notification = request;
      return { id: 42 };
    },
  });

  const id = await callbacks.pushNotification(
    HostPushNotificationRequest.enc({
      text: "Hello!",
      deeplink: undefined,
      scheduledAt: undefined,
    }),
  );

  assert.equal(id, 42);
  assert.deepEqual(notification, {
    text: "Hello!",
    deeplink: undefined,
    scheduledAt: undefined,
  });
});
