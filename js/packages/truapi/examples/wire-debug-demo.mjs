// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Runnable demo of the payload-blind observe hook + wire debugger.
 *
 *   bun examples/wire-debug-demo.mjs
 *
 * Wires `createTransport({ observe })` to a `createWireDebugger` and fires a real
 * `account.getAccount` call over an in-memory provider, printing the readable
 * per-frame trace and the grouped `WireTrace` for the op. No host, no network —
 * a scripted in-memory provider stands in for the host and echoes one response.
 */

import {
  createTransport,
  createWireDebugger,
  createMethodNameMap,
} from "../src/index.ts";
import { createClient } from "../src/generated/client.ts";
import { encodeWireMessage } from "../src/transport.ts";
import {
  Result,
  CallError,
  indexedTaggedUnion,
} from "../src/scale.ts";
import * as T from "../src/generated/types.ts";
import * as W from "../src/generated/wire-table.ts";

// An in-memory provider: captures outbound frames and lets us push a reply.
function inMemoryProvider() {
  let onFrame = () => {};
  return {
    provider: {
      postMessage() {},
      subscribe(cb) {
        onFrame = cb;
        return () => {};
      },
      subscribeClose() {
        return () => {};
      },
      dispose() {},
    },
    reply(frame) {
      onFrame(frame);
    },
  };
}

const { provider, reply } = inMemoryProvider();

// A method-name map so trace lines read `account.getAccount`, not `id=22`.
// Service names come from the generated client's own namespaces.
const probe = createTransport(provider, { observe: () => {} });
const methodNames = createMethodNameMap(W, Object.keys(createClient(probe)));

// The debugger: prints each frame and groups them into per-requestId traces.
const dbg = createWireDebugger({
  methodNames,
  sink: (line) => console.log("  " + line),
});

const transport = createTransport(provider, { observe: dbg.observe });
const client = createClient(transport);

console.log("account.getAccount round trip:\n");

// Fire the request. The outbound frame is observed synchronously, so the
// debugger already holds this op's trace (and its requestId) right after.
const pending = client.account.getAccount({
  productAccountId: { dotNsIdentifier: "demo.dot", derivationIndex: 0 },
});
const { requestId } = dbg.traces().at(-1);

// The "host" answers on the same requestId with a success response.
const responsePayload = indexedTaggedUnion({
  V1: [
    0,
    Result(T.HostAccountGetResponse, CallError(T.VersionedHostAccountGetError)),
  ],
}).enc({
  tag: "V1",
  value: { success: true, value: { account: { publicKey: "0x" + "11".repeat(32) } } },
});
const frame = encodeWireMessage({
  requestId,
  payload: { id: W.ACCOUNT_GET_ACCOUNT.response, value: responsePayload },
});
if (frame.isErr()) throw frame.error;
reply(frame.value);

const result = await pending;

console.log(`\nresult: ${result.isOk() ? "Ok" : "Err"}`);
if (result.isOk()) {
  console.log(`  account.publicKey = ${result.value.account.publicKey}`);
}

// The grouped trace: one op, both directions, under one id.
const trace = dbg.trace(requestId);
console.log(`\nWireTrace ${trace.requestId} — ${trace.frames.length} frames:`);
for (const f of trace.frames) {
  const arrow = f.direction === "out" ? "→" : "←";
  const name = methodNames.get(f.frameId)?.method ?? `id=${f.frameId}`;
  console.log(`  ${arrow} ${f.role.padEnd(8)} ${name}  (${f.byteLength}B)`);
}
console.log(
  "\nThe same requestId keys the product-sdk span (HostOpEvent.correlationId),\n" +
    "so this wire trace and a product telemetry span line up under one id.",
);
