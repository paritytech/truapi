import assert from "node:assert/strict";

import { createTransport } from "../src/client.ts";
import { Result, indexedTaggedUnion, _void } from "../src/scale.ts";
import { createClient } from "../src/generated/client.ts";
import * as T from "../src/generated/types.ts";
import * as W from "../src/generated/wire-table.ts";
import { encodeWireMessage } from "../src/transport.ts";
import { createWireDebugger } from "../src/debug.ts";

/**
 * END-TO-END CORRELATION CHECK
 * ----------------------------
 * Demonstrates the unified-id story locally with no network/host:
 *
 *   1. A product-side "span" wraps a host op (here, a handshake) and records a
 *      correlationId — modelling `@parity/product-sdk-logger`'s `withSpan`.
 *   2. The truapi transport's `observe` hook (consumed by `createWireDebugger`)
 *      records every wire frame keyed on its `requestId`.
 *   3. The span ADOPTS the wire `requestId` surfaced by the first observed
 *      outbound frame — so the product span and the truapi dispatcher trace are
 *      keyed on the SAME id.
 *
 * The assertion is the whole point: span.correlationId === wireTrace.requestId.
 * One id, followable from the product vantage through the wire to the host.
 */

function unwrap(result, message) {
  return result.match(
    (value) => value,
    (error) => assert.fail(`${message}: ${error.message}`),
  );
}

function providerFixture() {
  const sent = [];
  let listener = () => {};
  return {
    sent,
    provider: {
      postMessage(m) {
        sent.push(m);
      },
      subscribe(cb) {
        listener = cb;
        return () => {};
      },
      subscribeClose() {
        return () => {};
      },
      dispose() {},
    },
    receive(m) {
      listener(m);
    },
  };
}

function handshakeResponsePayload(value) {
  return indexedTaggedUnion({
    V1: [0, Result(_void, T.HostHandshakeError)],
  }).enc({ tag: "V1", value });
}

/**
 * Minimal stand-in for product-sdk-logger's `withSpan`: times the body, lets it
 * adopt a wire id, and emits one structured event on settle.
 */
async function withSpan(options, body) {
  const events = options.sink;
  let correlationId = `psdk-${Date.now().toString(36)}`;
  const controls = {
    setCorrelationId: (id) => {
      correlationId = id;
    },
  };
  const start = Date.now();
  try {
    const result = await body(controls);
    events.push({
      op: options.op,
      outcome: "ok",
      correlationId,
      durationMs: Date.now() - start,
    });
    return result;
  } catch (error) {
    events.push({
      op: options.op,
      outcome: "error",
      correlationId,
      durationMs: Date.now() - start,
    });
    throw error;
  }
}

{
  const fixture = providerFixture();
  const dbg = createWireDebugger({ sink: () => {} });
  const transport = createTransport(fixture.provider, { observe: dbg.observe });
  const client = createClient(transport);

  const productEvents = [];

  // The product op: a span around a host call. The body adopts the wire id from
  // the debugger's most recent trace as soon as the request frame goes out.
  const opPromise = withSpan(
    { op: "handshake", sink: productEvents },
    async (controls) => {
      const response = client.system.handshake();
      // The outbound frame has already been observed synchronously; adopt its
      // requestId so the product span shares the wire id.
      const [latest] = dbg.traces().slice(-1);
      controls.setCorrelationId(latest.requestId);
      return response;
    },
  );

  // Host replies on the same wire id.
  const frame = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: {
        id: W.SYSTEM_HANDSHAKE.response,
        value: handshakeResponsePayload({ success: true, value: undefined }),
      },
    }),
    "encode handshake response",
  );
  fixture.receive(frame);
  await opPromise;

  // THE CHECK: one id ties the product span to the wire/dispatcher trace.
  const span = productEvents[0];
  const wireTrace = dbg.trace(span.correlationId);

  assert.equal(span.op, "handshake");
  assert.equal(span.outcome, "ok");
  assert.equal(span.correlationId, "p:1");
  assert.ok(wireTrace, "the product span's id must resolve to a wire trace");
  assert.equal(wireTrace.requestId, span.correlationId);
  // The wire trace shows both directions of the same op.
  assert.equal(wireTrace.frames.length, 2);
  assert.equal(wireTrace.frames[0].direction, "out");
  assert.equal(wireTrace.frames[1].direction, "in");

  console.log(
    `e2e correlation: product span op=${span.op} ` +
      `correlationId=${span.correlationId} ↔ wire trace ` +
      `(${wireTrace.frames.length} frames) under the SAME id ✓`,
  );
}

console.log("e2e-correlation test passed");
