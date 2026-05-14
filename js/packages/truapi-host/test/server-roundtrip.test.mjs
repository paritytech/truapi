// End-to-end smoke test for the @parity/truapi-host dispatcher.
//
// Wires a host server and a @parity/truapi client to a shared in-memory
// provider duo and asserts a request method round-trips, including the
// versioned envelope wrap/unwrap on both sides. The host handler is
// responsible for matching on the request's version tag and returning the
// versioned response envelope; the dispatcher is a thin SCALE codec wrapper.

import assert from "node:assert/strict";

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
 * just the methods it exercises.
 */
function makeStubHandlers(partial) {
  const stub = new Proxy(
    {},
    {
      get(_, prop) {
        return async () => {
          throw new Error(`unimplemented stub: ${String(prop)}`);
        };
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
  const accountStub = new Proxy(
    {},
    {
      get(_, prop) {
        if (prop === "getAccount") {
          return async (ctx, request) => {
            observed = { request, requestId: ctx.requestId };
            assert.equal(request.tag, "V1");
            return {
              tag: "V1",
              value: {
                success: true,
                value: { account: { publicKey: "0x" + "01".repeat(32) } },
              },
            };
          };
        }
        if (prop === "connectionStatusSubscribe") return () => () => {};
        return async () => {
          throw new Error(`unimplemented account stub: ${String(prop)}`);
        };
      },
    },
  );

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
  assert.deepEqual(observed.request, { tag: "V1", value: expectedRequest });
  assert.equal(typeof observed.requestId, "string");

  server.dispose();
  transport.dispose();
}

console.log("server-roundtrip: ok");
