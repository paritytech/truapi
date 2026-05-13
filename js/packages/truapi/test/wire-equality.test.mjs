// Wire byte-equality smoke test.
//
// `encodeWireMessage` must produce the same bytes as the Rust
// `ProtocolMessage` encoder for the canonical reference vectors. The Rust side
// pins these in `crates/truapi-server/src/frame.rs` (`mod tests`). We compute
// them here independently and compare.
//
// Run via: `npm test` (which loops over `test/*.test.mjs`) or `bun
// test/wire-equality.test.mjs` directly. No external test framework: bare
// `assert` is enough for the round-trip and the loop in package.json keeps
// future *.test.mjs files from being silently skipped.
//
// This file imports the *source* TS modules directly. Bun transpiles them on
// the fly, so the test does not depend on `npm run build` having run first.

import assert from "node:assert/strict";
import { decodeWireMessage, encodeWireMessage } from "../src/transport.ts";
import { str } from "../src/scale.ts";
import * as W from "../src/generated/wire-table.ts";

function toHex(u) {
  return Array.from(u)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function expectedWire(tagId, valueBytes) {
  const reqId = str.enc("p:1");
  const out = new Uint8Array(reqId.length + 1 + valueBytes.length);
  out.set(reqId, 0);
  out[reqId.length] = tagId;
  out.set(valueBytes, reqId.length + 1);
  return out;
}

/** Return the successful result value or fail the assertion with context. */
function unwrap(result, message) {
  return result.match(
    (value) => value,
    (error) => assert.fail(`${message}: ${error.message}`),
  );
}

// 1) handshake_request, discriminant = 0
{
  const inner = new Uint8Array([0x00, 0x01]); // V1 variant + codec_version=1
  const encoded = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: { id: W.SYSTEM_HANDSHAKE.request, value: inner },
    }),
    "encode handshake_request",
  );
  assert.equal(
    toHex(encoded),
    toHex(expectedWire(0, inner)),
    "handshake_request encoding mismatch with Rust reference",
  );
}

// 2) account_get_request, discriminant = 22, payload = V1(("foo", 0u32))
// This is the same vector pinned in `tests/snapshots/golden-account-get.bin`.
{
  const inner = new Uint8Array([
    0x00, // V1 variant
    ...str.enc("foo"), // compact-len + utf8
    0x00,
    0x00,
    0x00,
    0x00, // u32 = 0 LE
  ]);
  const encoded = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: { id: W.ACCOUNT_GET_ACCOUNT.request, value: inner },
    }),
    "encode account_get_request",
  );
  const expected = expectedWire(22, inner);
  assert.equal(
    toHex(encoded),
    toHex(expected),
    "account_get_request encoding mismatch with Rust reference",
  );
  // Also assert the absolute byte sequence matches the Rust golden fixture.
  const golden = "0c703a3116000c666f6f00000000";
  assert.equal(
    toHex(encoded),
    golden,
    `golden frame mismatch: expected ${golden}, got ${toHex(encoded)}`,
  );
}

// 3) round-trip
{
  const inner = new Uint8Array([0x00, 0x42, 0xab, 0xcd]);
  const encoded = unwrap(
    encodeWireMessage({
      requestId: "p:1",
      payload: { id: W.LOCAL_STORAGE_READ.request, value: inner },
    }),
    "encode local_storage_read_request",
  );
  const decoded = unwrap(
    decodeWireMessage(encoded),
    "decode local_storage_read_request",
  );
  assert.equal(decoded.requestId, "p:1");
  assert.equal(decoded.payload.id, W.LOCAL_STORAGE_READ.request);
  assert.equal(toHex(decoded.payload.value), toHex(inner));
}

// 4) invalid outbound discriminant must surface as Err.
{
  const result = encodeWireMessage({
    requestId: "p:1",
    payload: { id: 256, value: new Uint8Array() },
  });
  assert.ok(result.isErr(), "encode of id 256 should be Err");
  assert.match(result.error.message, /Invalid wire discriminant/);
}

// 5) truncated frame (no discriminant byte) must surface as Err.
{
  const truncated = str.enc("p:1"); // just the requestId, nothing after.
  const result = decodeWireMessage(truncated);
  assert.ok(result.isErr(), "decode of truncated frame should be Err");
  assert.match(result.error.message, /missing discriminant byte/);
}

// 6) 32 KiB requestId exercises scanStrEnd mode-2 (4-byte compact-len prefix).
// Mirrors Rust `max_length_request_id_mode_two_round_trips` so both sides agree
// the high-shift branch in scanStrEnd is correct.
{
  const longId = "y".repeat(32 * 1024);
  const inner = new Uint8Array([0x00, 0xab, 0xcd]);
  const encoded = unwrap(
    encodeWireMessage({
      requestId: longId,
      payload: { id: W.ACCOUNT_GET_ACCOUNT.request, value: inner },
    }),
    "encode long-id account_get_request",
  );
  // Confirm the encoder actually emitted a mode-2 prefix; otherwise the test
  // is silently exercising mode-0/1 and the assertion is hollow.
  assert.equal(encoded[0] & 0b11, 0b10, "expected mode-2 compact-len prefix");
  const decoded = unwrap(
    decodeWireMessage(encoded),
    "decode long-id account_get_request",
  );
  assert.equal(decoded.requestId, longId);
  assert.equal(decoded.payload.id, W.ACCOUNT_GET_ACCOUNT.request);
  assert.equal(toHex(decoded.payload.value), toHex(inner));
}

console.log("all 6 wire-equality tests passed");
