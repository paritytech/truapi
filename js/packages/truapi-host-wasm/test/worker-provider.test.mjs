import assert from "node:assert/strict";
import test from "node:test";

import {
  HostPushNotificationRequest,
  HostPushNotificationResponse,
} from "../../truapi/dist/index.js";
import { createWasmRawCallbacks } from "../dist/index.js";
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
    pushNotification: async () => ({ id: 0 }),
    devicePermission: async () => ({ granted: false }),
    remotePermission: async () => ({ granted: false }),
    featureSupported: async () => ({ supported: false }),
    read: async () => undefined,
    write: async () => {},
    clear: async () => {},
    ...overrides,
  };
}

function runtimeConfig(overrides = {}) {
  return {
    productId: "dotli.dot",
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

async function readyProvider(worker, options = {}) {
  const {
    createWebWorkerProvider: createProvider = createWebWorkerProvider,
    ...providerOptions
  } = options;
  const providerPromise = createProvider(worker, makeCallbacks(), {
    runtimeConfig: runtimeConfig(),
    ...providerOptions,
  });
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  return providerPromise;
}

test("createWebWorkerProvider advertises the full optional callback surface", async () => {
  // The generated adapter fills every optional callback with a default, so the
  // provider advertises the complete optional surface; `chainConnect` reflects
  // whether the host supplied a `connect` capability.
  const worker = new FakeWorker();
  const config = runtimeConfig();
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      connect: async () => ({
        send() {},
        // eslint-disable-next-line require-yield
        async *responses() {},
      }),
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
    optionalCallbacks: [
      "cancelNotification",
      "authStateChanged",
      "readStoredSession",
      "writeStoredSession",
      "clearStoredSession",
      "confirmUserAction",
      "submitPreimage",
    ],
    optionalSubscriptions: ["subscribeTheme", "lookupPreimage"],
    chainConnect: true,
  });

  worker.emit({ kind: "ready" });
  const provider = await providerPromise;
  assert.equal(typeof provider.disconnectSession, "function");
  assert.equal(typeof provider.cancelPairing, "function");
  assert.equal(typeof provider.notifySessionStoreChanged, "function");

  provider.dispose();
});

test("dev global setLogLevel updates every live worker provider", async () => {
  const previous = globalThis.__truapi;
  delete globalThis.__truapi;
  const firstWorker = new FakeWorker();
  const secondWorker = new FakeWorker();
  const first = await readyProvider(firstWorker);
  const second = await readyProvider(secondWorker);

  globalThis.__truapi.setLogLevel("debug");

  assert.deepEqual(firstWorker.messages.at(-1), {
    kind: "setLogLevel",
    level: "debug",
  });
  assert.deepEqual(secondWorker.messages.at(-1), {
    kind: "setLogLevel",
    level: "debug",
  });
  assert.equal(globalThis.__truapi.getLogLevel(), "debug");

  globalThis.__truapi.setLogLevel("off");
  first.dispose();
  second.dispose();
  if (previous === undefined) {
    delete globalThis.__truapi;
  } else {
    globalThis.__truapi = previous;
  }
});

test("dev global setLogLevel applies to providers created later", async () => {
  const previous = globalThis.__truapi;
  delete globalThis.__truapi;
  const moduleUrl = `../dist/web/create-worker-host-runtime.js?dev-global-${Date.now()}`;
  const { createWebWorkerProvider: freshCreateWebWorkerProvider } =
    await import(moduleUrl);

  assert.equal(typeof globalThis.__truapi.setLogLevel, "function");
  globalThis.__truapi.setLogLevel("trace");

  const firstWorker = new FakeWorker();
  const first = await readyProvider(firstWorker, {
    createWebWorkerProvider: freshCreateWebWorkerProvider,
  });
  first.dispose();

  const secondWorker = new FakeWorker();
  const second = await readyProvider(secondWorker, {
    createWebWorkerProvider: freshCreateWebWorkerProvider,
  });

  assert.equal(secondWorker.messages[0].kind, "init");
  assert.equal(secondWorker.messages[0].logLevel, "trace");
  assert.deepEqual(secondWorker.messages.at(-1), {
    kind: "setLogLevel",
    level: "trace",
  });

  second.dispose();
  globalThis.__truapi.setLogLevel("off");
  if (previous === undefined) {
    delete globalThis.__truapi;
  } else {
    globalThis.__truapi = previous;
  }
});

test("dev global setLogLevel persists the level to localStorage", async () => {
  const previousGlobal = globalThis.__truapi;
  const previousStorage = globalThis.localStorage;
  delete globalThis.__truapi;
  const store = new Map();
  globalThis.localStorage = {
    getItem: (key) => (store.has(key) ? store.get(key) : null),
    setItem: (key, value) => store.set(key, String(value)),
  };

  const worker = new FakeWorker();
  const provider = await readyProvider(worker);

  globalThis.__truapi.setLogLevel("debug");
  assert.equal(store.get("truapi:logLevel"), "debug");

  globalThis.__truapi.setLogLevel("off");
  assert.equal(store.get("truapi:logLevel"), "off");

  provider.dispose();
  globalThis.localStorage = previousStorage;
  if (previousGlobal === undefined) {
    delete globalThis.__truapi;
  } else {
    globalThis.__truapi = previousGlobal;
  }
});

test("worker provider resolves disconnect responses", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(worker, makeCallbacks(), {
    runtimeConfig: runtimeConfig(),
  });
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  const disconnect = provider.disconnectSession();
  const msg = worker.messages.at(-1);
  assert.equal(msg.kind, "disconnectSession");
  assert.equal(typeof msg.requestId, "number");

  worker.emit({
    kind: "disconnectSessionResponse",
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
      clearStoredSession: async () => {
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
    name: "clearStoredSession",
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

test("worker provider forwards authStateChanged callback requests", async () => {
  const worker = new FakeWorker();
  const states = [];
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      authStateChanged: (state) => {
        states.push(state);
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
    name: "authStateChanged",
    args: [
      {
        tag: "Connected",
        value: {
          publicKey: new Uint8Array([1, 2]),
          liteUsername: "alice",
        },
      },
    ],
  });
  await settle();

  assert.deepEqual(states, [
    {
      tag: "Connected",
      value: {
        publicKey: new Uint8Array([1, 2]),
        liteUsername: "alice",
      },
    },
  ]);
  assert.deepEqual(worker.messages.at(-1), {
    kind: "callbackResponse",
    requestId: 3,
    ok: true,
    value: undefined,
  });

  provider.dispose();
});

test("worker provider posts cancelPairing to the worker", async () => {
  const worker = new FakeWorker();
  const provider = await readyProvider(worker);

  provider.cancelPairing();

  assert.deepEqual(worker.messages.at(-1), { kind: "cancelPairing" });
  provider.dispose();
});

test("worker provider posts notifySessionStoreChanged to the worker", async () => {
  const worker = new FakeWorker();
  const provider = await readyProvider(worker);

  provider.notifySessionStoreChanged();

  assert.deepEqual(worker.messages.at(-1), {
    kind: "notifySessionStoreChanged",
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
      // Manual async iterables whose `return()` records disposal; the provider
      // disposes subscriptions and closes chain connections on a worker fault.
      subscribeTheme: () => ({
        [Symbol.asyncIterator]() {
          return this;
        },
        next: () => new Promise(() => {}),
        return: async () => {
          subscriptionDisposes += 1;
          return { done: true, value: undefined };
        },
      }),
      connect: async () => ({
        send() {},
        responses: () => ({
          [Symbol.asyncIterator]() {
            return this;
          },
          next: () => new Promise(() => {}),
          return: async () => {
            chainCloses += 1;
            return { done: true, value: undefined };
          },
        }),
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
    name: "subscribeTheme",
    payload: null,
  });
  worker.emit({ kind: "chainConnectStart", connId: 1, genesisHash: "0xab" });
  await settle();
  await settle();

  const closes = [];
  provider.subscribeClose((error) => closes.push(error));

  worker.emitError("boom");
  await settle();
  await settle();

  assert.equal(worker.terminated, true);
  assert.equal(subscriptionDisposes, 1);
  assert.equal(chainCloses, 1);
  assert.equal(closes.length, 1);
  assert.match(closes[0].message, /boom/);

  // The fault teardown is terminal; a second fault is a no-op.
  worker.emitError("again");
  assert.equal(closes.length, 1);

  let lateClose = null;
  provider.subscribeClose((error) => {
    lateClose = error;
  });
  assert.ok(lateClose instanceof Error);
  assert.match(lateClose.message, /boom/);
});

test("worker fatalError during init rejects provider creation", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(worker, makeCallbacks(), {
    runtimeConfig: runtimeConfig(),
  });

  worker.emit({ kind: "fatalError", error: "bad wasm" });

  await assert.rejects(providerPromise, /worker init reported error: bad wasm/);
  assert.equal(worker.terminated, true);
});

test("worker frameError after init closes the provider", async () => {
  const worker = new FakeWorker();
  const provider = await readyProvider(worker);
  const closes = [];
  provider.subscribeClose((error) => closes.push(error));

  worker.emit({ kind: "frameError", error: "bad frame" });

  assert.equal(worker.terminated, true);
  assert.equal(closes.length, 1);
  assert.match(closes[0].message, /worker frame error: bad frame/);

  let lateClose = null;
  provider.subscribeClose((error) => {
    lateClose = error;
  });
  assert.ok(lateClose instanceof Error);
});

test("worker provider routes payload-carrying subscriptions by name", async () => {
  const worker = new FakeWorker();
  const keys = [];
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      lookupPreimage: async function* (key) {
        keys.push(key);
        yield { success: true, value: new Uint8Array([1]) };
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
    name: "lookupPreimage",
    payload: new Uint8Array([9, 9]),
  });

  await settle();
  await settle();
  assert.deepEqual(keys, [new Uint8Array([9, 9])]);
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
      lookupPreimage: () => {
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
      lookupPreimage: () => {
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
    name: "lookupPreimage",
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

  const encoded = await callbacks.pushNotification(
    HostPushNotificationRequest.enc({
      text: "Hello!",
      deeplink: undefined,
      scheduledAt: undefined,
    }),
  );

  assert.equal(HostPushNotificationResponse.dec(encoded).id, 42);
  assert.deepEqual(notification, {
    text: "Hello!",
    deeplink: undefined,
    scheduledAt: undefined,
  });
});
