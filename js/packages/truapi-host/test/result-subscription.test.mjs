// End-to-end test for `ResultSubscription` methods: the handler emits one
// item then errors with a typed reason; the dispatcher forwards both an
// inbound `receive` frame and an `interrupt` frame carrying the encoded
// reason; the client surfaces the reason via `SubscriptionError.reason`.

import assert from "node:assert/strict";

import { createTransport, createClient } from "../../truapi/src/index.ts";
import { createTrUApiServer } from "../src/index.ts";

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
    },
    b: {
      postMessage(message) {
        for (const listener of [...aListeners]) listener(message);
      },
      subscribe(callback) {
        bListeners.add(callback);
        return () => bListeners.delete(callback);
      },
    },
  };
}

function makeStubHandlers(partial) {
  const stub = new Proxy(
    {},
    {
      get(_, prop) {
        return () => {
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

await new Promise((resolveTest, rejectTest) => {
  const { a, b } = makeProviderPair();
  const transport = createTransport(a);
  const client = createClient(transport);

  const paymentStub = {
    balanceSubscribe(ctx, request) {
      assert.equal(typeof ctx.requestId, "string");
      assert.equal(request.tag, "V1");
      return {
        subscribe(observer) {
          queueMicrotask(() => {
            observer.next?.({ tag: "V1", value: { available: 100n } });
            queueMicrotask(() => {
              observer.error?.({
                reason: { tag: "V1", value: { tag: "PermissionDenied" } },
              });
            });
          });
          return { unsubscribe: () => {}, subscriptionId: "" };
        },
      };
    },
  };

  const server = createTrUApiServer(b, makeStubHandlers({ payment: paymentStub }));
  const received = [];
  client.payment.balanceSubscribe().subscribe({
    next(item) {
      received.push(item);
    },
    error(error) {
      try {
        assert.equal(received.length, 1);
        assert.equal(received[0].available, 100n);
        assert.equal(error.reason?.tag, "PermissionDenied");
      } catch (e) {
        server.dispose();
        transport.dispose();
        rejectTest(e);
        return;
      }
      server.dispose();
      transport.dispose();
      resolveTest();
    },
    complete() {
      server.dispose();
      transport.dispose();
      rejectTest(new Error("did not expect complete"));
    },
  });
});

console.log("result-subscription: ok");
