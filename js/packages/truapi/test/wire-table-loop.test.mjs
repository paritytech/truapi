// Programmatic wire-equality loop.
//
// `wire-equality.test.mjs` exercises a handful of hand-picked frames. This file
// iterates every generated numeric frame id and asserts the codec round-trips a
// sentinel payload and produces the expected byte layout for each.
//
// Run via: `bun run test`.

import assert from "node:assert/strict";
import { decodeWireMessage, encodeWireMessage } from "../src/transport.ts";
import * as W from "../src/generated/wire-table.ts";
import { str } from "../src/scale.ts";

function toHex(u) {
  return Array.from(u)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function expectedWire(reqId, tagId, valueBytes) {
  const idBytes = str.enc(reqId);
  const out = new Uint8Array(idBytes.length + 1 + valueBytes.length);
  out.set(idBytes, 0);
  out[idBytes.length] = tagId;
  out.set(valueBytes, idBytes.length + 1);
  return out;
}

function unwrap(result, message) {
  if (result.isErr()) throw new Error(`${message}: ${result.error.message}`);
  return result.value;
}

const frames = Object.entries(W).flatMap(([method, ids]) =>
  Object.entries(ids).map(([kind, id]) => ({ method, kind, id })),
);
assert.ok(frames.length > 0, "wire frame constants must not be empty");

// Use a per-id sentinel payload so any cross-talk between ids surfaces as a
// concrete byte mismatch rather than a silent equality.
let checked = 0;
for (const { method, kind, id } of frames) {
  const sentinel = new Uint8Array([id, 0xa5, ~id & 0xff, 0x5a]);
  const message = {
    requestId: `r:${id}`,
    payload: { id, value: sentinel },
  };
  const label = `${method}.${kind}`;
  const encoded = unwrap(encodeWireMessage(message), `encode ${label}`);
  const expected = expectedWire(`r:${id}`, id, sentinel);
  assert.equal(toHex(encoded), toHex(expected), `encode mismatch for ${label}`);

  const decoded = unwrap(decodeWireMessage(encoded), `decode ${label}`);
  assert.equal(decoded.requestId, `r:${id}`, `requestId mismatch for ${label}`);
  assert.equal(decoded.payload.id, id, `id mismatch for ${label}`);
  assert.equal(
    toHex(decoded.payload.value),
    toHex(sentinel),
    `payload mismatch for ${label}`,
  );
  checked++;
}

console.log(`programmatic wire-table loop: ${checked} frame ids round-tripped`);
