// Smoke test that `createNodeWasmProvider` instantiates the WASM core,
// returns a usable `Provider`, and disposes cleanly without leaking
// resources back to the caller.

import assert from "node:assert/strict";
import test from "node:test";

import { createNodeWasmProvider } from "../dist/index.js";

function makeCallbacks() {
  const unavailable = (name) => async () => {
    throw new Error(`${name} unavailable`);
  };
  const noopSubscribe = () => () => {};
  return {
    navigateTo: async () => {},
    pushNotification: async () => {},
    devicePermission: async () => false,
    remotePermission: async () => false,
    featureSupported: async () => new Uint8Array(),
    localStorageRead: async () => undefined,
    localStorageWrite: async () => {},
    localStorageClear: async () => {},
    accountGet: unavailable("accountGet"),
    accountGetAlias: unavailable("accountGetAlias"),
    accountCreateProof: unavailable("accountCreateProof"),
    getLegacyAccounts: unavailable("getLegacyAccounts"),
    accountConnectionStatusSubscribe: noopSubscribe,
    getUserId: unavailable("getUserId"),
    signPayload: unavailable("signPayload"),
    signRaw: unavailable("signRaw"),
    statementStoreSubscribe: noopSubscribe,
    statementStoreSubmit: unavailable("statementStoreSubmit"),
    statementStoreCreateProof: unavailable("statementStoreCreateProof"),
    preimageLookupSubscribe: noopSubscribe,
    dispose: () => {},
  };
}

test("createNodeWasmProvider returns a usable Provider", async () => {
  const provider = await createNodeWasmProvider(makeCallbacks());
  assert.equal(typeof provider.postMessage, "function");
  assert.equal(typeof provider.subscribe, "function");
  assert.equal(typeof provider.dispose, "function");

  // Subscribe and immediately unsubscribe to exercise the listener
  // bookkeeping without needing a valid frame.
  const unsubscribe = provider.subscribe(() => {});
  unsubscribe();

  provider.dispose();
});

test("createNodeWasmProvider dispose is idempotent", async () => {
  const provider = await createNodeWasmProvider(makeCallbacks());
  provider.dispose();
  // Second call must not throw.
  provider.dispose();
});
