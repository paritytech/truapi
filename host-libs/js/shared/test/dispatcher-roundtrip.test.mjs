// Smoke test that the dispatcher re-export from @parity/host-shared
// routes inbound request frames to a registered handler and emits a
// response frame back through the provider, end-to-end.

import assert from "node:assert/strict";
import test from "node:test";

import { encodeWireMessage, decodeWireMessage } from "@parity/truapi";
import { createHostServer } from "../dist/index.js";

function makeRecordingProvider() {
  const listeners = new Set();
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
      dispose() {
        listeners.clear();
      },
    },
    deliver(message) {
      for (const listener of [...listeners]) listener(message);
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
