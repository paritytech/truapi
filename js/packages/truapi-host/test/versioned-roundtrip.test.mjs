// Round-trip tests for the wrapper-tag method shapes the host generator
// has to handle distinctly: a no-param method (V1 unit request), a method
// whose handler signature carries the versioned wrapper tag explicitly
// (`signPayload`), and a method whose Ok side is Rust `()` (statement
// store `submit`).

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

// No-param method (`account.getUserId`). The Rust trait declares
// `_request: HostGetUserIdRequest` whose V1 variant is unit, so the host
// generator omits the handler's `request` parameter and the dispatcher
// still validates the inbound versioned envelope.
{
  const { a, b } = makeProviderPair();
  const transport = createTransport(a);
  const client = createClient(transport);

  let ctxRequestId;
  const accountStub = new Proxy(
    {},
    {
      get(_, prop) {
        if (prop === "getUserId") {
          return async (ctx) => {
            ctxRequestId = ctx.requestId;
            return {
              tag: "V1",
              value: {
                success: true,
                value: { primaryUsername: "alice.dot" },
              },
            };
          };
        }
        return async () => {
          throw new Error(`unimplemented account stub: ${String(prop)}`);
        };
      },
    },
  );

  const server = createTrUApiServer(b, makeStubHandlers({ account: accountStub }));
  const result = await client.account.getUserId();
  assert.ok(
    result.isOk(),
    `expected getUserId to succeed: ${JSON.stringify(result.error ?? null)}`,
  );
  assert.equal(result.value.primaryUsername, "alice.dot");
  assert.equal(typeof ctxRequestId, "string");

  server.dispose();
  transport.dispose();
}

// V1 wrapper-tag method (`signing.signPayload`). The handler receives the
// tagged request `{ tag: 'V1', value: ... }` and must return a tagged
// response of the same version. Verifies the full envelope encode/decode
// path the issue's acceptance criteria call out as wrapper-tag methods.
{
  const { a, b } = makeProviderPair();
  const transport = createTransport(a);
  const client = createClient(transport);

  const sampleRequest = {
    account: { dotNsIdentifier: "alice.dot", derivationIndex: 0 },
    blockHash: "0x" + "00".repeat(32),
    blockNumber: "0x10",
    era: "0x00",
    genesisHash: "0x" + "11".repeat(32),
    method: "0x1234",
    nonce: "0x01",
    specVersion: "0x0100",
    tip: "0x00",
    transactionVersion: "0x04",
    signedExtensions: [],
    version: 4,
  };
  const sampleSignature = "0x" + "ab".repeat(32);

  let observedRequest;
  const signingStub = new Proxy(
    {},
    {
      get(_, prop) {
        if (prop === "signPayload") {
          return async (_ctx, request) => {
            observedRequest = request;
            assert.equal(request.tag, "V1");
            return {
              tag: "V1",
              value: { success: true, value: { signature: sampleSignature } },
            };
          };
        }
        return async () => {
          throw new Error(`unimplemented signing stub: ${String(prop)}`);
        };
      },
    },
  );

  const server = createTrUApiServer(b, makeStubHandlers({ signing: signingStub }));
  const result = await client.signing.signPayload(sampleRequest);
  assert.ok(
    result.isOk(),
    `expected signPayload to succeed: ${JSON.stringify(result.error ?? null)}`,
  );
  assert.equal(result.value.signature, sampleSignature);
  assert.equal(observedRequest.tag, "V1");
  // SCALE decode reifies optional fields, so compare key-by-key on the
  // input set rather than deepEqual on the whole shape.
  for (const [key, expected] of Object.entries(sampleRequest)) {
    assert.deepEqual(
      observedRequest.value[key],
      expected,
      `signPayload request.value.${key} mismatch`,
    );
  }

  server.dispose();
  transport.dispose();
}

// Method whose Rust `Ok` is `()` (`statementStore.submit`). The host
// generator must encode `S.Result(S._void, ErrorCodec)` inside the
// versioned envelope; the handler returns `{ value: undefined }` on success.
{
  const { a, b } = makeProviderPair();
  const transport = createTransport(a);
  const client = createClient(transport);

  const sampleStatement = {
    proof: {
      tag: "Sr25519",
      value: {
        signature: "0x" + "00".repeat(64),
        signer: "0x" + "11".repeat(32),
      },
    },
    topics: [],
  };

  const statementStoreStub = new Proxy(
    {},
    {
      get(_, prop) {
        if (prop === "submit") {
          return async (_ctx, request) => {
            assert.equal(request.tag, "V1");
            return {
              tag: "V1",
              value: { success: true, value: undefined },
            };
          };
        }
        return async () => {
          throw new Error(`unimplemented statementStore stub: ${String(prop)}`);
        };
      },
    },
  );

  const server = createTrUApiServer(
    b,
    makeStubHandlers({ statementStore: statementStoreStub }),
  );
  const result = await client.statementStore.submit(sampleStatement);
  assert.ok(
    result.isOk(),
    `expected submit to succeed: ${JSON.stringify(result.error ?? null)}`,
  );

  server.dispose();
  transport.dispose();
}

console.log("versioned-roundtrip: ok");
