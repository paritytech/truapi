// Sanity test that the worker-protocol module is importable and exports
// what `createWebWorkerProvider` (from @parity/truapi-host-wasm/web) expects. The
// real web worker entry-point loads a browser-only WASM bundle, so we
// cannot boot it under Node; this test verifies the wire-shape of the
// shared protocol contract instead.

import assert from "node:assert/strict";
import test from "node:test";

import * as shared from "../dist/index.js";
import * as workerProtocol from "../dist/worker-protocol.js";

test("worker-protocol exports callback-name tables", () => {
  assert.deepEqual(Object.keys(workerProtocol), [
    "CALLBACK_NAMES",
    "OPTIONAL_CALLBACK_NAMES",
  ]);
  assert.ok(workerProtocol.CALLBACK_NAMES.includes("navigateTo"));
  assert.ok(workerProtocol.OPTIONAL_CALLBACK_NAMES.includes("readSession"));
  assert.ok(!workerProtocol.OPTIONAL_CALLBACK_NAMES.includes("navigateTo"));
});

test("@parity/truapi-host-wasm exposes the documented surface", () => {
  // Dispatcher re-export from @parity/truapi-host.
  assert.equal(typeof shared.createHostServer, "function");
  assert.equal(typeof shared.toFlatResponsePayload, "function");
  assert.equal(typeof shared.toResponsePayload, "function");

  // WASM provider helpers.
  assert.equal(typeof shared.createWasmProvider, "function");
  assert.equal(typeof shared.createNodeWasmProvider, "function");
  assert.equal(typeof shared.createUnavailableCallbacks, "function");
});
