import assert from "node:assert/strict";
import test from "node:test";

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

async function settle() {
  await new Promise((resolve) => setImmediate(resolve));
}

test("createWebWorkerProvider advertises only supplied optional hooks", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(
    worker,
    makeCallbacks({
      clearSession: async () => {},
      readSession: async () => new Uint8Array([1]),
      subscribeSessionStore: () => () => {},
      preimageLookupSubscribe: () => () => {},
      chainConnect: () => ({ send: () => {}, close: () => {} }),
    }),
    {
      debug: true,
      runtimeConfig: { productId: "dotli" },
    },
  );

  worker.emit({ kind: "loaded" });
  assert.equal(worker.messages.length, 1);
  assert.deepEqual(worker.messages[0], {
    kind: "init",
    debug: true,
    runtimeConfig: { productId: "dotli" },
    optionalCallbacks: ["readSession", "clearSession"],
    optionalSubscriptions: [
      "sessionStoreSubscribe",
      "preimageLookupSubscribe",
    ],
    chainConnect: true,
  });

  worker.emit({ kind: "ready" });
  const provider = await providerPromise;
  assert.equal(typeof provider.disconnect, "function");

  provider.dispose();
});

test("worker provider resolves disconnect responses", async () => {
  const worker = new FakeWorker();
  const providerPromise = createWebWorkerProvider(worker, makeCallbacks());
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  const provider = await providerPromise;

  const disconnect = provider.disconnect();
  const msg = worker.messages.at(-1);
  assert.equal(msg.kind, "disconnect");
  assert.equal(typeof msg.requestId, "number");

  worker.emit({ kind: "disconnectResponse", requestId: msg.requestId, ok: true });
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
