// Regression test for the start/stop race: when a `stop` frame arrives
// before the handler's async `start` resolves, the dispatcher must invoke
// the eventual cleanup instead of registering an orphaned subscription.
//
// The test feeds raw wire frames directly into `createHostServer` so the
// timing is deterministic.

import assert from "node:assert/strict";

import { encodeWireMessage } from "../../truapi/src/index.ts";
import { createHostServer } from "../src/index.ts";

function makeRecordingProvider() {
  let listener;
  const outbound = [];
  return {
    provider: {
      postMessage(message) {
        outbound.push(message);
      },
      subscribe(callback) {
        listener = callback;
        return () => {
          listener = undefined;
        };
      },
    },
    deliver(message) {
      if (listener) listener(message);
    },
    outbound,
  };
}

// Async start, stop arrives synchronously after start — the slot is still
// pending when stop is processed. The dispatcher must invoke the cleanup
// once start resolves, leaving no orphaned subscription.
{
  const { provider, deliver, outbound } = makeRecordingProvider();
  let cleanupCalled = 0;
  let resolveStart;
  const startPromise = new Promise((r) => {
    resolveStart = r;
  });
  const entry = {
    kind: "subscription",
    ids: { start: 200, stop: 201, interrupt: 202, receive: 203 },
    async start(_bytes, _ctx, _port) {
      await startPromise;
      return () => {
        cleanupCalled += 1;
      };
    },
  };
  const server = createHostServer(provider, [entry]);

  const startFrame = encodeWireMessage({
    requestId: "req-race-1",
    payload: { id: 200, value: new Uint8Array() },
  });
  assert.ok(startFrame.isOk(), "encode start frame");
  const stopFrame = encodeWireMessage({
    requestId: "req-race-1",
    payload: { id: 201, value: new Uint8Array() },
  });
  assert.ok(stopFrame.isOk(), "encode stop frame");

  deliver(startFrame.value);
  deliver(stopFrame.value);

  // Start is still pending; cleanup has not been invoked yet.
  assert.equal(cleanupCalled, 0, "cleanup must not run before start resolves");

  resolveStart();
  // Flush microtasks for the awaited promise + .then handlers.
  await new Promise((r) => queueMicrotask(r));
  await new Promise((r) => queueMicrotask(r));

  assert.equal(cleanupCalled, 1, "cleanup must run exactly once after the late stop");
  assert.equal(outbound.length, 0, "no frames should have been emitted");
  server.dispose();
}

// Sync start with a sync provider — the original buggy path deferred
// start via `Promise.resolve().then`, so a stop frame arriving in the
// same tick orphaned the handler. With the fix, sync start completes
// before stop is processed, and the subscription is torn down cleanly.
{
  const { provider, deliver } = makeRecordingProvider();
  let cleanupCalled = 0;
  const entry = {
    kind: "subscription",
    ids: { start: 210, stop: 211, interrupt: 212, receive: 213 },
    start(_bytes, _ctx, _port) {
      return () => {
        cleanupCalled += 1;
      };
    },
  };
  const server = createHostServer(provider, [entry]);

  const startFrame = encodeWireMessage({
    requestId: "req-race-2",
    payload: { id: 210, value: new Uint8Array() },
  });
  const stopFrame = encodeWireMessage({
    requestId: "req-race-2",
    payload: { id: 211, value: new Uint8Array() },
  });

  deliver(startFrame.value);
  deliver(stopFrame.value);

  assert.equal(cleanupCalled, 1, "sync start+stop must invoke cleanup once");
  server.dispose();
}

// Dispose while a start is still pending: the dispatcher should let the
// resolving start invoke its own cleanup and not leave dangling state.
{
  const { provider, deliver } = makeRecordingProvider();
  let cleanupCalled = 0;
  let resolveStart;
  const startPromise = new Promise((r) => {
    resolveStart = r;
  });
  const entry = {
    kind: "subscription",
    ids: { start: 220, stop: 221, interrupt: 222, receive: 223 },
    async start(_bytes, _ctx, _port) {
      await startPromise;
      return () => {
        cleanupCalled += 1;
      };
    },
  };
  const server = createHostServer(provider, [entry]);
  const startFrame = encodeWireMessage({
    requestId: "req-race-3",
    payload: { id: 220, value: new Uint8Array() },
  });
  deliver(startFrame.value);

  server.dispose();
  resolveStart();
  await new Promise((r) => queueMicrotask(r));
  await new Promise((r) => queueMicrotask(r));

  assert.equal(cleanupCalled, 1, "dispose during pending must still trigger cleanup");
}

console.log("start-stop-race: ok");
