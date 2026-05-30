// Verify `createElectronProvider` adapts an Electron-style port into a
// TrUAPI Provider: subscribers receive inbound binary frames, outbound
// frames flow back through `port.postMessage`, and `dispose` closes the
// port and clears listeners.

import assert from "node:assert/strict";
import test from "node:test";

import { createElectronProvider } from "../dist/electron/index.js";

function makeFakePort() {
  const messageListeners = new Set();
  const closeListeners = new Set();
  const sent = [];
  const offCalls = [];
  let closed = false;
  return {
    sent,
    offCalls,
    isClosed: () => closed,
    deliverMessage(data) {
      for (const listener of [...messageListeners]) listener({ data });
    },
    deliverClose() {
      for (const listener of [...closeListeners]) listener();
    },
    api: {
      postMessage(message) {
        sent.push(message);
      },
      on(event, handler) {
        if (event === "message") messageListeners.add(handler);
        else if (event === "close") closeListeners.add(handler);
        return this;
      },
      off(event, handler) {
        offCalls.push(event);
        if (event === "message") messageListeners.delete(handler);
        else if (event === "close") closeListeners.delete(handler);
        return this;
      },
      start() {},
      close() {
        closed = true;
      },
    },
  };
}

test("createElectronProvider forwards inbound frames to subscribers and outbound frames to the port", () => {
  const fake = makeFakePort();
  const provider = createElectronProvider({ port: fake.api });

  const received = [];
  const unsubscribe = provider.subscribe((message) => {
    received.push(message);
  });

  const inbound = new Uint8Array([10, 20, 30]);
  fake.deliverMessage(inbound);
  // Non-binary frames are ignored.
  fake.deliverMessage({ type: "ignored" });

  assert.equal(received.length, 1);
  assert.deepEqual(Array.from(received[0]), [10, 20, 30]);

  const outbound = new Uint8Array([1, 2]);
  provider.postMessage(outbound);
  assert.equal(fake.sent.length, 1);
  assert.deepEqual(Array.from(fake.sent[0]), [1, 2]);

  unsubscribe();
  fake.deliverMessage(new Uint8Array([99]));
  // Unsubscribed, length should not grow.
  assert.equal(received.length, 1);

  provider.dispose();
  assert.equal(fake.isClosed(), true, "dispose closes the underlying port");
});

test("createElectronProvider notifies close subscribers when the port closes", () => {
  const fake = makeFakePort();
  const provider = createElectronProvider({ port: fake.api });

  const closes = [];
  provider.subscribeClose((error) => closes.push(error));

  fake.deliverClose();
  assert.equal(closes.length, 1);
  assert.ok(closes[0] instanceof Error);

  provider.dispose();
});

test("dispose removes both port listeners and blocks further traffic", () => {
  const fake = makeFakePort();
  const provider = createElectronProvider({ port: fake.api });

  const received = [];
  provider.subscribe((message) => received.push(message));

  provider.dispose();
  assert.deepEqual(
    [...fake.offCalls].sort(),
    ["close", "message"],
    "dispose detaches both the message and close handlers",
  );

  // postMessage after dispose is a no-op.
  provider.postMessage(new Uint8Array([1, 2]));
  assert.equal(fake.sent.length, 0, "no frames sent after dispose");

  // Inbound frames after dispose never reach subscribers.
  fake.deliverMessage(new Uint8Array([3, 4]));
  assert.equal(received.length, 0, "no frames delivered after dispose");
});

test("a peer-initiated close detaches port listeners and blocks postMessage", () => {
  const fake = makeFakePort();
  const provider = createElectronProvider({ port: fake.api });

  provider.subscribeClose(() => {});
  fake.deliverClose();

  assert.deepEqual(
    [...fake.offCalls].sort(),
    ["close", "message"],
    "peer close detaches both handlers",
  );

  // postMessage after a peer-initiated close is a no-op.
  provider.postMessage(new Uint8Array([5]));
  assert.equal(fake.sent.length, 0, "no frames sent after a peer close");

  provider.dispose();
});
