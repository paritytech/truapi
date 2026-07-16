import assert from "node:assert/strict";

import { createTransport } from "../src/client.ts";
import { Result, indexedTaggedUnion, _void } from "../src/scale.ts";
import { createClient } from "../src/generated/client.ts";
import * as T from "../src/generated/types.ts";
import * as W from "../src/generated/wire-table.ts";
import { encodeWireMessage } from "../src/transport.ts";
import { createMethodNameMap, createWireDebugger } from "../src/debug.ts";

/** Return the successful result value or fail the assertion with context. */
function unwrap(result, message) {
  return result.match(
    (value) => value,
    (error) => assert.fail(`${message}: ${error.message}`),
  );
}

/** Create an in-memory provider plus helpers for injecting frames. */
function providerFixture() {
  const sent = [];
  let listener = () => {};
  return {
    sent,
    provider: {
      postMessage(message) {
        sent.push(message);
      },
      subscribe(callback) {
        listener = callback;
        return () => {};
      },
      subscribeClose() {
        return () => {};
      },
      dispose() {},
    },
    receive(message) {
      listener(message);
    },
  };
}

/** Encode a V1 host-handshake response result payload. */
function handshakeResponsePayload(value) {
  return indexedTaggedUnion({
    V1: [0, Result(_void, T.HostHandshakeError)],
  }).enc({ tag: "V1", value });
}

// observe fires for an outbound request frame with the wire requestId + role.
{
  const fixture = providerFixture();
  const frames = [];
  const transport = createTransport(fixture.provider, {
    observe: (f) => frames.push(f),
  });
  const client = createClient(transport);

  void client.account.getAccount({
    productAccountId: { dotNsIdentifier: "foo", derivationIndex: 0 },
  });

  assert.equal(frames.length, 1);
  assert.equal(frames[0].direction, "out");
  assert.equal(frames[0].role, "request");
  assert.equal(frames[0].requestId, "p:1");
  assert.equal(frames[0].frameId, W.ACCOUNT_GET_ACCOUNT.request);
  assert.ok(frames[0].byteLength > 0);
  // No decoded payload is ever exposed — keys are id/shape only.
  assert.deepEqual(
    Object.keys(frames[0]).sort(),
    [
      "byteLength",
      "direction",
      "frameId",
      "requestId",
      "role",
      "timestamp",
    ].sort(),
  );
}

// observe fires for the inbound response under the SAME requestId — this is the
// correlation spine: one id, both directions.
{
  const fixture = providerFixture();
  const frames = [];
  const transport = createTransport(fixture.provider, {
    observe: (f) => frames.push(f),
  });
  const client = createClient(transport);

  const response = client.system.handshake();
  const frame = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: {
        id: W.SYSTEM_HANDSHAKE.response,
        value: handshakeResponsePayload({ success: true, value: undefined }),
      },
    }),
    "encode handshake_response",
  );
  fixture.receive(frame);
  await response;

  const out = frames.find((f) => f.direction === "out");
  const inbound = frames.find((f) => f.direction === "in");
  assert.ok(out, "expected an outbound frame");
  assert.ok(inbound, "expected an inbound frame");
  assert.equal(out.requestId, inbound.requestId);
  assert.equal(out.role, "handshake");
  assert.equal(inbound.role, "handshake");
}

// A throwing observer never breaks message delivery.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider, {
    observe: () => {
      throw new Error("observer blew up");
    },
  });
  const client = createClient(transport);
  // Should not throw despite the faulty observer.
  void client.system.handshake();
  assert.equal(fixture.sent.length, 1);
}

// Zero-cost when unset: no observer ⇒ behaviour identical to baseline.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider);
  const client = createClient(transport);
  void client.system.handshake();
  assert.equal(fixture.sent.length, 1);
}

// createWireDebugger groups frames per requestId — the trace a product span's
// correlationId looks up.
{
  const fixture = providerFixture();
  const lines = [];
  const dbg = createWireDebugger({ sink: (line) => lines.push(line) });
  const transport = createTransport(fixture.provider, { observe: dbg.observe });
  const client = createClient(transport);

  const response = client.system.handshake();
  const frame = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: {
        id: W.SYSTEM_HANDSHAKE.response,
        value: handshakeResponsePayload({ success: true, value: undefined }),
      },
    }),
    "encode handshake_response",
  );
  fixture.receive(frame);
  await response;

  const trace = dbg.trace("p:1");
  assert.ok(trace, "expected a trace for p:1");
  assert.equal(trace.requestId, "p:1");
  // One outbound request + one inbound response under the same id.
  assert.equal(trace.frames.length, 2);
  assert.equal(trace.frames[0].direction, "out");
  assert.equal(trace.frames[1].direction, "in");
  assert.ok(lines.length >= 2);
  assert.ok(lines[0].includes("p:1"));
}

// forward relays every frame onward (e.g. to a host panel) while the debugger
// keeps its own per-id traces.
{
  const fixture = providerFixture();
  const forwarded = [];
  const dbg = createWireDebugger({
    sink: () => {},
    forward: (f) => forwarded.push(f),
  });
  const transport = createTransport(fixture.provider, { observe: dbg.observe });
  const client = createClient(transport);

  void client.system.handshake();
  assert.equal(forwarded.length, 1);
  assert.equal(forwarded[0].requestId, "p:1");
}

// Subscription lifecycle roles: start → receive → stop, all under one
// requestId — the multi-frame trace where per-id grouping earns its keep.
{
  const fixture = providerFixture();
  const frames = [];
  const transport = createTransport(fixture.provider, {
    observe: (f) => frames.push(f),
  });
  const client = createClient(transport);

  const items = [];
  const sub = client.theme.subscribe().subscribe({
    next: (item) => items.push(item),
  });

  // The outbound `_start` frame is observed synchronously with role "start".
  assert.equal(frames.length, 1);
  assert.equal(frames[0].direction, "out");
  assert.equal(frames[0].role, "start");
  assert.equal(frames[0].frameId, W.THEME_SUBSCRIBE.start);
  const requestId = frames[0].requestId;
  assert.equal(requestId, sub.subscriptionId);

  // Host pushes one item: inbound `_receive` under the SAME id, role "receive".
  const receiveFrame = unwrap(
    encodeWireMessage({
      requestId,
      payload: {
        id: W.THEME_SUBSCRIBE.receive,
        value: T.VersionedHostThemeSubscribeItem.enc({
          tag: "V1",
          value: { name: { tag: "Default" }, variant: "Dark" },
        }),
      },
    }),
    "encode theme receive",
  );
  fixture.receive(receiveFrame);

  assert.equal(items.length, 1);
  assert.equal(items[0].variant, "Dark");
  assert.equal(frames.length, 2);
  assert.equal(frames[1].direction, "in");
  assert.equal(frames[1].role, "receive");
  assert.equal(frames[1].requestId, requestId);

  // Unsubscribe emits the outbound `_stop`, still under the same id.
  sub.unsubscribe();
  assert.equal(frames.length, 3);
  assert.equal(frames[2].direction, "out");
  assert.equal(frames[2].role, "stop");
  assert.equal(frames[2].frameId, W.THEME_SUBSCRIBE.stop);
  assert.equal(frames[2].requestId, requestId);

  // One id, three lifecycle roles, both directions.
  assert.deepEqual(
    frames.map((f) => `${f.direction}:${f.role}`),
    ["out:start", "in:receive", "out:stop"],
  );
}

// frameId → "service.method" reverse map, derived from the generated
// wire-table plus the client's own service names — no hardcoded list.
{
  const fixture = providerFixture();
  const transport = createTransport(fixture.provider, { observe: () => {} });
  const services = Object.keys(createClient(transport));
  const names = createMethodNameMap(W, services);

  // Simple two-word const: service prefix + single-word method.
  assert.deepEqual(names.get(W.ACCOUNT_GET_ACCOUNT.request), {
    method: "account.getAccount",
    kind: "request",
  });
  assert.equal(names.get(W.ACCOUNT_GET_ACCOUNT.response).method, "account.getAccount");

  // Multi-word service prefix must split as the client does:
  // LOCAL_STORAGE_READ → localStorage.read, not local.storageRead.
  assert.deepEqual(names.get(W.LOCAL_STORAGE_READ.request), {
    method: "localStorage.read",
    kind: "request",
  });

  // Subscription groups map all four lifecycle ids to one method.
  for (const kind of ["start", "stop", "receive", "interrupt"]) {
    const info = names.get(W.THEME_SUBSCRIBE[kind]);
    assert.equal(info.method, "theme.subscribe", `theme.subscribe ${kind}`);
    assert.equal(info.kind, kind);
  }

  // Every wire-table group resolves to a dotted service.method name.
  for (const [constName, group] of Object.entries(W)) {
    if (group === null || typeof group !== "object") continue;
    for (const id of Object.values(group)) {
      if (typeof id !== "number") continue;
      const info = names.get(id);
      assert.ok(info, `unmapped frame id ${id} (${constName})`);
      assert.match(info.method, /^[a-z][a-zA-Z]*\.[a-z][a-zA-Z0-9]*$/i, constName);
    }
  }
}

// The debugger's sink lines carry method names when a map is supplied.
{
  const fixture = providerFixture();
  const lines = [];
  const probeTransport = createTransport(fixture.provider, { observe: () => {} });
  const map = createMethodNameMap(W, Object.keys(createClient(probeTransport)));

  const dbg = createWireDebugger({ sink: (line) => lines.push(line), methodNames: map });
  const transport = createTransport(fixture.provider, { observe: dbg.observe });
  const client = createClient(transport);
  client.account.getAccount({
    productAccountId: { dotNsIdentifier: "demo", derivationIndex: 0 },
  });
  assert.equal(lines.length, 1);
  assert.ok(
    lines[0].includes("request account.getAccount"),
    `line should carry the method name: ${lines[0]}`,
  );
}

console.log("observe-hook tests passed");
