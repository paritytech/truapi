// Smoke test that the dispatcher re-export from @parity/truapi-host-shared
// routes inbound request frames to a registered handler and emits a
// response frame back through the provider, end-to-end.

import assert from "node:assert/strict";
import test from "node:test";

import { encodeWireMessage, decodeWireMessage } from "@parity/truapi";
import { createHostServer } from "../dist/index.js";

function makeRecordingProvider() {
  const listeners = new Set();
  const closeListeners = new Set();
  const sent = [];
  return {
    sent,
    provider: {
      postMessage(bytes) {
        sent.push(bytes);
      },
      subscribe(callback) {
        listeners.add(callback);
        return () => listeners.delete(callback);
      },
      subscribeClose(callback) {
        closeListeners.add(callback);
        return () => closeListeners.delete(callback);
      },
      dispose() {
        listeners.clear();
        closeListeners.clear();
      },
    },
    deliver(message) {
      for (const listener of [...listeners]) listener(message);
    },
    triggerClose(error) {
      for (const listener of [...closeListeners]) listener(error);
    },
  };
}

test("createHostServer dispatches a request id to the matching entry and emits a response", async () => {
  const requestId = 7;
  const responseId = 8;
  const { provider, sent, deliver } = makeRecordingProvider();

  const entries = [
    {
      kind: "request",
      ids: { request: requestId, response: responseId },
      async handle(ctx, payload) {
        assert.equal(typeof ctx.requestId, "string");
        // Echo with one extra byte so the test asserts that the right
        // bytes flowed through.
        const out = new Uint8Array(payload.length + 1);
        out.set(payload);
        out[payload.length] = 42;
        return out;
      },
    },
  ];

  const server = createHostServer(provider, entries);

  const inboundFrame = encodeWireMessage({
    requestId: "req-1",
    payload: { id: requestId, value: new Uint8Array([1, 2, 3]) },
  });
  assert.ok(inboundFrame.isOk(), "inbound frame must encode");
  deliver(inboundFrame.value);

  // Allow the handler microtask + send to resolve.
  await new Promise((r) => setImmediate(r));

  assert.equal(sent.length, 1, "exactly one response emitted");
  const decoded = decodeWireMessage(sent[0]);
  assert.ok(decoded.isOk(), "response frame must decode");
  assert.equal(decoded.value.requestId, "req-1");
  assert.equal(decoded.value.payload.id, responseId);
  assert.deepEqual(
    Array.from(decoded.value.payload.value),
    [1, 2, 3, 42],
    "payload should echo + extra byte",
  );

  server.dispose();
});

test("a rejecting request handler triggers onRequestHandlerError and emits no frame", async () => {
  const { provider, sent, deliver } = makeRecordingProvider();

  const errors = [];
  const entries = [
    {
      kind: "request",
      ids: { request: 7, response: 8 },
      async handle() {
        throw new Error("handler boom");
      },
    },
  ];
  const server = createHostServer(provider, entries, {
    onRequestHandlerError: (ids, error, ctx) => {
      errors.push({ ids, error, ctx });
    },
  });

  const frame = encodeWireMessage({
    requestId: "req-err",
    payload: { id: 7, value: new Uint8Array([1]) },
  });
  assert.ok(frame.isOk());
  deliver(frame.value);
  await new Promise((r) => setImmediate(r));

  assert.equal(errors.length, 1, "onRequestHandlerError fired once");
  assert.equal(errors[0].error.message, "handler boom");
  assert.deepEqual(errors[0].ids, { request: 7, response: 8 });
  assert.equal(errors[0].ctx.requestId, "req-err");
  assert.equal(sent.length, 0, "no response frame on handler rejection");

  server.dispose();
});

test("a frame with an unregistered id triggers onUnknownFrame", () => {
  const { provider, sent, deliver } = makeRecordingProvider();

  const unknown = [];
  const server = createHostServer(provider, [], {
    onUnknownFrame: (payload) => unknown.push(payload),
  });

  const frame = encodeWireMessage({
    requestId: "req-unknown",
    payload: { id: 200, value: new Uint8Array([9, 9]) },
  });
  assert.ok(frame.isOk());
  deliver(frame.value);

  assert.equal(unknown.length, 1, "onUnknownFrame fired once");
  assert.equal(unknown[0].id, 200);
  assert.deepEqual(Array.from(unknown[0].value), [9, 9]);
  assert.equal(sent.length, 0, "no frame emitted for an unknown id");

  server.dispose();
});

test("a truncated/garbage buffer is dropped without throwing or sending", () => {
  const { provider, sent, deliver } = makeRecordingProvider();

  const unknown = [];
  const handlerErrors = [];
  const server = createHostServer(provider, [], {
    onUnknownFrame: (payload) => unknown.push(payload),
    onRequestHandlerError: (_ids, error) => handlerErrors.push(error),
  });

  // A compact-length prefix that promises more bytes than exist.
  assert.doesNotThrow(() => deliver(new Uint8Array([0xff, 0xff, 0xff, 0xff])));

  assert.equal(sent.length, 0, "no frame emitted for a garbage buffer");
  assert.equal(unknown.length, 0, "decode failure is not an unknown frame");
  assert.equal(
    handlerErrors.length,
    0,
    "decode failure does not reach handlers",
  );

  server.dispose();
});

test("firing the provider close callback disposes the server", async () => {
  const recording = makeRecordingProvider();
  const { provider, sent, deliver, triggerClose } = recording;

  let handled = 0;
  const entries = [
    {
      kind: "request",
      ids: { request: 7, response: 8 },
      async handle(_ctx, payload) {
        handled += 1;
        return payload;
      },
    },
  ];
  const server = createHostServer(provider, entries);

  triggerClose(new Error("provider gone"));

  const frame = encodeWireMessage({
    requestId: "after-close",
    payload: { id: 7, value: new Uint8Array([1]) },
  });
  assert.ok(frame.isOk());
  deliver(frame.value);
  await new Promise((r) => setImmediate(r));

  assert.equal(handled, 0, "inbound frames are ignored after close");
  assert.equal(sent.length, 0, "no frames emitted after close");

  // dispose remains idempotent after a close-driven teardown.
  server.dispose();
});
