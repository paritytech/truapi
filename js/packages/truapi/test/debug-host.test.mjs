// Headless debug host (mock/forward engine) on the current core — routing,
// marking, correlation, teardown. Mock entries are byte-level (encoded with
// the generated codecs by the caller); the router, decisions, observe seam,
// loudness defaults, and the dispose-time upstream stop are all pinned here.

import assert from "node:assert/strict";

import {
  createTransport,
  createClient,
  createDebugHost,
  scale as S,
  HostHandshakeError,
  VersionedHostThemeSubscribeItem,
} from "../src/index.ts";
import * as T from "../src/generated/types.ts";
import * as W from "../src/generated/wire-table.ts";

function makeProviderPair() {
  const aListeners = new Set();
  const bListeners = new Set();
  return {
    a: {
      postMessage(m) { for (const cb of [...bListeners]) cb(m); },
      subscribe(cb) { aListeners.add(cb); return () => aListeners.delete(cb); },
      dispose() { aListeners.clear(); },
    },
    b: {
      postMessage(m) { for (const cb of [...aListeners]) cb(m); },
      subscribe(cb) { bListeners.add(cb); return () => bListeners.delete(cb); },
      dispose() { bListeners.clear(); },
    },
  };
}

const HANDSHAKE_RESPONSE = S.indexedTaggedUnion({
  V1: [0, S.Result(S._void, HostHandshakeError)],
});
const ACCOUNT_GET_RESPONSE_CODEC = S.indexedTaggedUnion({
  V1: [0, S.Result(T.HostAccountGetResponse, T.HostAccountGetError)],
});
const ACCOUNT_GET_RESPONSE_OK = (publicKey) =>
  ACCOUNT_GET_RESPONSE_CODEC.enc({
    tag: "V1",
    value: { success: true, value: { account: { publicKey } } },
  });

const MOCK_KEY = `0x${"22".repeat(32)}`;
const themeItem = (variant) =>
  VersionedHostThemeSubscribeItem.enc({
    tag: "V1",
    value: { name: { tag: "Default" }, variant },
  });

const accountEntry = {
  kind: "request",
  ids: W.ACCOUNT_GET_ACCOUNT,
  handle(ctx) {
    assert.equal(typeof ctx.requestId, "string");
    return ACCOUNT_GET_RESPONSE_OK(MOCK_KEY);
  },
};

function timeout(ms) {
  return new Promise((_, reject) =>
    setTimeout(() => reject(new Error(`timeout after ${ms}ms`)), ms),
  );
}

// ---------------------------------------------------------------------------
// 1. Headless: mock entry answers through real codecs + loud unhandled +
//    throwing-observer isolation.
// ---------------------------------------------------------------------------
{
  const { a, b } = makeProviderPair();
  const decisions = [];
  const debugHost = createDebugHost({
    provider: b,
    entries: [accountEntry],
    observe: () => {
      throw new Error("boom"); // a throwing observer must never break routing
    },
    onDecision: (d) => decisions.push(d),
  });
  const transport = createTransport(a);
  const client = createClient(transport);

  const result = await client.account.getAccount({
    productAccountId: { dotNsIdentifier: "demo.dot", derivationIndex: 0 },
  });
  assert.ok(result.isOk());
  assert.equal(result.value.account.publicKey, MOCK_KEY);

  const mockDecisions = decisions.filter((d) => d.tier === "mock");
  assert.deepEqual(
    mockDecisions.map((d) => [d.frame.direction, d.frame.role, d.method]),
    [
      ["in", "request", "account.getAccount"],
      ["out", "response", "account.getAccount"],
    ],
  );
  assert.equal(mockDecisions[0].frame.requestId, mockDecisions[1].frame.requestId);
  assert.deepEqual(
    Object.keys(mockDecisions[0].frame).sort(),
    ["byteLength", "direction", "frameId", "requestId", "role", "timestamp"],
  );

  await assert.rejects(
    Promise.race([client.system.handshake(), timeout(100)]),
    /timeout/,
  );
  const unhandled = decisions.filter((d) => d.tier === "unhandled");
  assert.equal(unhandled.length, 1);
  assert.equal(unhandled[0].method, "system.handshake");

  debugHost.dispose();
  transport.dispose();
  console.log("debug-host mock entry + unhandled + isolation: ok");
}

// ---------------------------------------------------------------------------
// 2. Forward: unclaimed frames reach the upstream verbatim with the SAME
//    requestId; entry claims win over the forward pipe. The upstream is
//    itself a headless createDebugHost — dogfooding.
// ---------------------------------------------------------------------------
{
  const { a: productEnd, b: debugEnd } = makeProviderPair();
  const { a: forwardPipe, b: upstreamEnd } = makeProviderPair();

  const upstreamSeen = [];
  createDebugHost({
    provider: upstreamEnd,
    entries: [
      {
        kind: "request",
        ids: W.SYSTEM_HANDSHAKE,
        handle(ctx) {
          upstreamSeen.push(ctx.requestId);
          return HANDSHAKE_RESPONSE.enc({
            tag: "V1",
            value: { success: true, value: undefined },
          });
        },
      },
    ],
  });

  const decisions = [];
  const debugHost = createDebugHost({
    provider: debugEnd,
    forward: forwardPipe,
    entries: [accountEntry],
    onDecision: (d) => decisions.push(d),
  });
  const transport = createTransport(productEnd);
  const client = createClient(transport);

  const handshake = await client.system.handshake();
  assert.ok(handshake.isOk());
  const forwarded = decisions.filter((d) => d.tier === "forward");
  assert.deepEqual(
    forwarded.map((d) => [d.frame.direction, d.frame.role]),
    [["in", "request"], ["out", "response"]],
  );
  assert.equal(upstreamSeen.length, 1);
  assert.equal(upstreamSeen[0], forwarded[0].frame.requestId); // across the hop

  const result = await client.account.getAccount({
    productAccountId: { dotNsIdentifier: "demo.dot", derivationIndex: 0 },
  });
  assert.ok(result.isOk());
  assert.equal(upstreamSeen.length, 1); // the mocked method never left
  assert.equal(decisions.filter((d) => d.tier === "mock").length, 2);

  debugHost.dispose();
  transport.dispose();
  console.log("debug-host forward + entry precedence (dogfooded upstream): ok");
}

// ---------------------------------------------------------------------------
// 3. Mocked subscription lifecycle under one requestId; stop runs cleanup.
// ---------------------------------------------------------------------------
{
  const { a, b } = makeProviderPair();
  const decisions = [];
  let cleanedUp = false;
  createDebugHost({
    provider: b,
    entries: [
      {
        kind: "subscription",
        ids: W.THEME_SUBSCRIBE,
        start(ctx, _payload, port) {
          queueMicrotask(() => {
            port.sendReceive(themeItem("Dark"));
            port.sendReceive(themeItem("Light"));
          });
          return () => {
            cleanedUp = true;
          };
        },
      },
    ],
    onDecision: (d) => decisions.push(d),
  });
  const transport = createTransport(a);
  const client = createClient(transport);

  const received = [];
  let sub;
  await new Promise((resolve) => {
    sub = client.theme.subscribe().subscribe({
      next(item) {
        received.push(item.variant);
        if (received.length === 2) queueMicrotask(resolve);
      },
    });
  });
  sub.unsubscribe();

  assert.deepEqual(received, ["Dark", "Light"]);
  assert.ok(cleanedUp, "entry cleanup ran on stop");
  assert.deepEqual(
    decisions.map((d) => [d.tier, d.frame.direction, d.frame.role]),
    [
      ["mock", "in", "start"],
      ["mock", "out", "receive"],
      ["mock", "out", "receive"],
      ["mock", "in", "stop"],
    ],
  );
  assert.equal(new Set(decisions.map((d) => d.frame.requestId)).size, 1);

  transport.dispose();
  console.log("debug-host mocked subscription lifecycle: ok");
}

// ---------------------------------------------------------------------------
// 4. Dispose with a live FORWARDED subscription: the debug host sends the
//    stop frame upstream — the real host's cleanup runs (S4 leak fixed).
// ---------------------------------------------------------------------------
{
  const { a: productEnd, b: debugEnd } = makeProviderPair();
  const { a: forwardPipe, b: upstreamEnd } = makeProviderPair();

  let upstreamCleanup = false;
  createDebugHost({
    provider: upstreamEnd,
    entries: [
      {
        kind: "subscription",
        ids: W.THEME_SUBSCRIBE,
        start(_ctx, _payload, port) {
          port.sendReceive(themeItem("Dark"));
          return () => {
            upstreamCleanup = true;
          };
        },
      },
    ],
  });

  const debugHost = createDebugHost({ provider: debugEnd, forward: forwardPipe });
  const transport = createTransport(productEnd);
  const client = createClient(transport);

  const received = [];
  client.theme.subscribe().subscribe({ next: (i) => received.push(i.variant) });
  await new Promise((resolve) => setTimeout(resolve, 0)); // entry dispatch is async
  assert.equal(received.length, 1);
  assert.equal(upstreamCleanup, false);

  debugHost.dispose(); // must stop the forwarded subscription upstream
  assert.equal(upstreamCleanup, true, "upstream cleanup ran on debug-host dispose");

  transport.dispose();
  console.log("debug-host dispose stops forwarded subscriptions upstream: ok");
}

// ---------------------------------------------------------------------------
// 5. Default config is LOUD: unhandled + undecodable both warn.
// ---------------------------------------------------------------------------
{
  const { a, b } = makeProviderPair();
  const warnings = [];
  const originalWarn = console.warn;
  console.warn = (m) => warnings.push(String(m));
  try {
    createDebugHost({ provider: b });
    const transport = createTransport(a);
    const client = createClient(transport);
    await assert.rejects(
      Promise.race([client.system.handshake(), timeout(100)]),
      /timeout/,
    );
    a.postMessage(new Uint8Array([0xff, 0xff, 0xff, 0xff]));
    assert.equal(warnings.length, 2);
    assert.match(warnings[0], /unhandled frame system\.handshake/);
    assert.match(warnings[0], /the caller will hang/);
    assert.match(warnings[1], /undecodable wire envelope/);
    transport.dispose();
  } finally {
    console.warn = originalWarn;
  }
  console.log("debug-host default-config loudness (unhandled + undecodable): ok");
}

// ---------------------------------------------------------------------------
// 6. Undecodable envelopes stay byte-transparent when a forward pipe exists;
//    a throwing forward pipe self-disposes instead of breaking the router;
//    duplicate entries are rejected at construction.
// ---------------------------------------------------------------------------
{
  const { a: productEnd, b: debugEnd } = makeProviderPair();
  const { a: forwardPipe, b: upstreamEnd } = makeProviderPair();
  const upstreamRaw = [];
  upstreamEnd.subscribe((m) => upstreamRaw.push(m));
  createDebugHost({ provider: debugEnd, forward: forwardPipe });
  const garbage = new Uint8Array([0xff, 0xff, 0xff, 0xff, 0xff]);
  productEnd.postMessage(garbage);
  assert.equal(upstreamRaw.length, 1);
  assert.deepEqual([...upstreamRaw[0]], [...garbage]);

  const { a: p2, b: d2 } = makeProviderPair();
  const deadForward = {
    postMessage() { throw new Error("pipe closed"); },
    subscribe() { return () => {}; },
    dispose() {},
  };
  createDebugHost({ provider: d2, forward: deadForward });
  const transport2 = createTransport(p2);
  const client2 = createClient(transport2);
  await assert.rejects(
    Promise.race([client2.system.handshake(), timeout(100)]),
    /timeout/,
  );
  transport2.dispose();

  assert.throws(
    () =>
      createDebugHost({
        provider: makeProviderPair().b,
        entries: [accountEntry, accountEntry],
      }),
    /duplicate entry/,
  );
  console.log("debug-host transparency + dead-pipe isolation + validation: ok");
}

// ---------------------------------------------------------------------------
// 7. Drift guard: the debug host resolves method names for EVERY service the
//    client exposes, and its dispose-time upstream stop is not theme-specific.
//    (Regression for the hand-maintained service list this port removed.)
// ---------------------------------------------------------------------------
{
  // A forwarded subscription on a NON-theme service must also be stopped
  // upstream on dispose — proving the ledger covers whatever codegen emits,
  // not one hardcoded example.
  const { a: productEnd, b: debugEnd } = makeProviderPair();
  const { a: forwardPipe, b: upstreamEnd } = makeProviderPair();

  let upstreamStopped = false;
  createDebugHost({
    provider: upstreamEnd,
    entries: [
      {
        kind: "subscription",
        ids: W.CHAT_LIST_SUBSCRIBE,
        start() {
          return () => { upstreamStopped = true; };
        },
      },
    ],
  });

  const debugHost = createDebugHost({ provider: debugEnd, forward: forwardPipe });
  const transport = createTransport(productEnd);
  const client = createClient(transport);

  client.chat.listSubscribe().subscribe({ next: () => {} });
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(upstreamStopped, false);
  debugHost.dispose();
  assert.equal(upstreamStopped, true, "dispose stops a non-theme forwarded subscription upstream");

  transport.dispose();
  console.log("debug-host dispose-stop covers all services (drift guard): ok");
}
