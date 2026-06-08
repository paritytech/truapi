// Smoke test that `createNodeWasmProvider` instantiates the WASM core,
// returns a usable `Provider`, and disposes cleanly without leaking
// resources back to the caller.

import assert from "node:assert/strict";
import test from "node:test";

import {
  encodeWireMessage,
  decodeWireMessage,
  VersionedHostFeatureSupportedRequest,
  HostFeatureSupportedResponse,
  GenericError,
  scale as S,
} from "@parity/truapi";
import { SYSTEM_FEATURE_SUPPORTED } from "@parity/truapi/wire-table";

import { createNodeWasmProvider } from "../dist/index.js";

function makeCallbacks(overrides = {}) {
  const noopSubscribe = () => () => {};
  return {
    navigateTo: async () => {},
    pushNotification: async () => 0,
    devicePermission: async () => false,
    remotePermission: async () => false,
    featureSupported: async () => false,
    localStorageRead: async () => undefined,
    localStorageWrite: async () => {},
    localStorageClear: async () => {},
    clearSession: async () => {},
    preimageLookupSubscribe: noopSubscribe,
    dispose: () => {},
    ...overrides,
  };
}

test("createNodeWasmProvider returns a usable Provider", async () => {
  const provider = await createNodeWasmProvider(makeCallbacks());
  assert.equal(typeof provider.postMessage, "function");
  assert.equal(typeof provider.subscribe, "function");
  assert.equal(typeof provider.disconnect, "function");
  assert.equal(typeof provider.dispose, "function");

  // Subscribe and immediately unsubscribe to exercise the listener
  // bookkeeping without needing a valid frame.
  const unsubscribe = provider.subscribe(() => {});
  unsubscribe();

  provider.dispose();
});

test("createNodeWasmProvider exposes core disconnect", async () => {
  let clears = 0;
  const provider = await createNodeWasmProvider(
    makeCallbacks({
      clearSession: async () => {
        clears += 1;
      },
    }),
  );

  await provider.disconnect();

  assert.equal(clears, 1);
  provider.dispose();
});

test("createNodeWasmProvider dispose is idempotent", async () => {
  const provider = await createNodeWasmProvider(makeCallbacks());
  provider.dispose();
  // Second call must not throw.
  provider.dispose();
});

test("createNodeWasmProvider round-trips a featureSupported request through the WASM core", async () => {
  const callbacks = makeCallbacks();
  callbacks.featureSupported = async () => true;
  const provider = await createNodeWasmProvider(callbacks);

  const frames = [];
  provider.subscribe((bytes) => frames.push(bytes));

  const payload = VersionedHostFeatureSupportedRequest.enc({
    tag: "V1",
    value: { tag: "Chain", value: { genesisHash: "0x00" } },
  });
  const inbound = encodeWireMessage({
    requestId: "rt-1",
    payload: { id: SYSTEM_FEATURE_SUPPORTED.request, value: payload },
  });
  assert.ok(inbound.isOk(), "request frame must encode");
  provider.postMessage(inbound.value);

  // Let the WASM dispatch + host callback + emit cycle settle.
  await new Promise((r) => setTimeout(r, 50));

  assert.equal(frames.length, 1, "exactly one response frame emitted");
  const decoded = decodeWireMessage(frames[0]);
  assert.ok(decoded.isOk(), "response frame must decode");
  assert.equal(decoded.value.requestId, "rt-1");
  assert.equal(decoded.value.payload.id, SYSTEM_FEATURE_SUPPORTED.response);

  const responseCodec = S.indexedTaggedUnion({
    V1: [0, S.Result(HostFeatureSupportedResponse, GenericError)],
  });
  const response = responseCodec.dec(decoded.value.payload.value);
  assert.deepEqual(response, {
    tag: "V1",
    value: { success: true, value: { supported: true } },
  });

  provider.dispose();
});

test("createNodeWasmProvider surfaces a rejected receiveFromProduct through subscribeClose", async () => {
  const provider = await createNodeWasmProvider(makeCallbacks());

  const closes = [];
  provider.subscribeClose((error) => closes.push(error));
  const frames = [];
  provider.subscribe((bytes) => frames.push(bytes));

  // A garbage buffer the core cannot decode rejects receiveFromProduct.
  provider.postMessage(new Uint8Array([0xff, 0xff, 0xff, 0xff, 0xff]));
  await new Promise((r) => setTimeout(r, 50));

  assert.equal(closes.length, 1, "close listener fires once on decode failure");
  assert.ok(closes[0] instanceof Error);
  assert.equal(frames.length, 0, "no response frame on a rejected frame");

  provider.dispose();
});
