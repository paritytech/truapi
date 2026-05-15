// End-to-end smoke test for the @parity/truapi-host dispatcher.
//
// Wires a host server and a @parity/truapi client to a shared in-memory
// provider duo and asserts a request method round-trips, including the
// versioned envelope wrap/unwrap on both sides. The host handler is
// responsible for matching on the request's version tag and returning the
// versioned response envelope; the dispatcher is a thin SCALE codec wrapper.

import assert from "node:assert/strict";
import { okAsync } from "neverthrow";

import { createTransport, createClient } from "../../truapi/src/index.ts";
import { createTrUApiServer } from "../src/index.ts";

/**
 * Build a synchronous in-memory provider pair. Messages posted through one
 * end arrive at the other end's listeners in the same tick. No multiplexing,
 * no batching, no ordering tricks.
 */
function makeProviderPair() {
  const aListeners = new Set();
  const bListeners = new Set();
  return {
    a: {
      postMessage(message) {
        for (const listener of [...bListeners]) listener(message);
      },
      subscribe(callback) {
        aListeners.add(callback);
        return () => aListeners.delete(callback);
      },
      dispose() {
        aListeners.clear();
      },
    },
    b: {
      postMessage(message) {
        for (const listener of [...aListeners]) listener(message);
      },
      subscribe(callback) {
        bListeners.add(callback);
        return () => bListeners.delete(callback);
      },
      dispose() {
        bListeners.clear();
      },
    },
  };
}

/**
 * Wrap a partial map of handlers with a Proxy-based stub for every other
 * service the generated `TrUApiHostHandlers` requires. Lets a test register
 * just the methods it exercises. Method stubs surface as per-version maps,
 * matching the generated handler shape (`{ v1(...), v2(...) }`).
 */
function makeStubHandlers(partial) {
  const versionStub = new Proxy(
    {},
    {
      get(_, version) {
        return () => {
          throw new Error(`unimplemented stub: ${String(version)}`);
        };
      },
    },
  );
  const stub = new Proxy(
    {},
    {
      get() {
        return versionStub;
      },
    },
  );
  const services = [
    "account",
    "chain",
    "chat",
    "entropy",
    "jsonRpc",
    "localStorage",
    "payment",
    "permissions",
    "preimage",
    "resourceAllocation",
    "signing",
    "statementStore",
    "system",
    "theme",
  ];
  const result = {};
  for (const name of services) {
    result[name] = partial[name] ?? stub;
  }
  return result;
}

// Request round-trip: client.account.getAccount → host handler → typed response.
{
  const { a, b } = makeProviderPair();
  const transport = createTransport(a);
  const client = createClient(transport);

  let observed;
  const accountStub = {
    getAccount: {
      v1(ctx, request) {
        observed = { request, requestId: ctx.requestId };
        return okAsync({ account: { publicKey: "0x" + "01".repeat(32) } });
      },
    },
    connectionStatusSubscribe: {
      v1: () => ({
        subscribe: () => ({ unsubscribe: () => {}, subscriptionId: "" }),
      }),
    },
  };

  const server = createTrUApiServer(b, makeStubHandlers({ account: accountStub }));

  const expectedRequest = {
    productAccountId: {
      dotNsIdentifier: "my-product.dot",
      derivationIndex: 0,
    },
  };
  const result = await client.account.getAccount(expectedRequest);
  assert.ok(
    result.isOk(),
    `expected getAccount to succeed: ${JSON.stringify(result.error ?? null)}`,
  );
  assert.deepEqual(result.value.account.publicKey, "0x" + "01".repeat(32));
  assert.deepEqual(observed.request, expectedRequest);
  assert.equal(typeof observed.requestId, "string");

  server.dispose();
  transport.dispose();
}

// Subscription round-trip: handler returns an ObservableLike; client receives
// items via the standard `subscribe({next, complete})` shape on its side. The
// host emits one item then calls `observer.complete()` to terminate.
await new Promise((resolveTest, rejectTest) => {
  const { a, b } = makeProviderPair();
  const transport = createTransport(a);
  const client = createClient(transport);

  let lastSent;
  const accountStub = {
    connectionStatusSubscribe: {
      v1(ctx) {
        assert.equal(typeof ctx.requestId, "string");
        return {
          subscribe(observer) {
            const item = "Connected";
            lastSent = item;
            queueMicrotask(() => {
              observer.next?.(item);
              queueMicrotask(() => observer.complete?.());
            });
            return { unsubscribe: () => {}, subscriptionId: "" };
          },
        };
      },
    },
  };

  const server = createTrUApiServer(b, makeStubHandlers({ account: accountStub }));

  const received = [];
  client.account.connectionStatusSubscribe().subscribe({
    next(value) {
      received.push(value);
    },
    error(error) {
      server.dispose();
      transport.dispose();
      rejectTest(new Error(`unexpected error: ${error.message}`));
    },
    complete() {
      try {
        assert.equal(received.length, 1);
        assert.deepEqual(received[0], lastSent);
        server.dispose();
        transport.dispose();
        resolveTest();
      } catch (error) {
        server.dispose();
        transport.dispose();
        rejectTest(error);
      }
    },
  });
});

console.log("server-roundtrip: ok");
