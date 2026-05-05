// Wire byte-equality smoke test.
//
// The TS `byteProtocolCodecAdapter` must produce the same bytes as the Rust
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

import assert from 'node:assert/strict';
import { byteProtocolCodecAdapter } from '../src/transport.ts';
import { str } from '../src/scale.ts';

function toHex(u) {
  return Array.from(u).map((b) => b.toString(16).padStart(2, '0')).join('');
}

function expectedWire(tagId, valueBytes) {
  const reqId = str.enc('p:1');
  const out = new Uint8Array(reqId.length + 1 + valueBytes.length);
  out.set(reqId, 0);
  out[reqId.length] = tagId;
  out.set(valueBytes, reqId.length + 1);
  return out;
}

// 1) handshake_request, discriminant = 0
{
  const inner = new Uint8Array([0x00, 0x01]); // V2 variant + codec_version=1
  const encoded = byteProtocolCodecAdapter.encode({
    requestId: 'p:1',
    payload: { tag: 'host_handshake_request', value: inner },
  });
  assert.equal(toHex(encoded), toHex(expectedWire(0, inner)),
    'handshake_request encoding mismatch with Rust reference');
}

// 2) account_get_request, discriminant = 22, payload = V2(("foo", 0u32))
// This is the same vector pinned in `tests/snapshots/golden-account-get.bin`.
{
  const inner = new Uint8Array([
    0x00, // V2 variant
    ...str.enc('foo'), // compact-len + utf8
    0x00, 0x00, 0x00, 0x00, // u32 = 0 LE
  ]);
  const encoded = byteProtocolCodecAdapter.encode({
    requestId: 'p:1',
    payload: { tag: 'host_account_get_request', value: inner },
  });
  const expected = expectedWire(22, inner);
  assert.equal(toHex(encoded), toHex(expected),
    'account_get_request encoding mismatch with Rust reference');
  // Also assert the absolute byte sequence matches the Rust golden fixture.
  const golden = '0c703a3116000c666f6f00000000';
  assert.equal(toHex(encoded), golden,
    `golden frame mismatch: expected ${golden}, got ${toHex(encoded)}`);
}

// 3) round-trip
{
  const inner = new Uint8Array([0x00, 0x42, 0xab, 0xcd]);
  const encoded = byteProtocolCodecAdapter.encode({
    requestId: 'p:1',
    payload: { tag: 'host_local_storage_read_request', value: inner },
  });
  const decoded = byteProtocolCodecAdapter.decode(encoded);
  assert.equal(decoded.requestId, 'p:1');
  assert.equal(decoded.payload.tag, 'host_local_storage_read_request');
  assert.equal(toHex(decoded.payload.value), toHex(inner));
}

// 4) unknown discriminant must throw
{
  const reqId = str.enc('p:1');
  const bytes = new Uint8Array(reqId.length + 1 + 4);
  bytes.set(reqId);
  bytes[reqId.length] = 250;
  bytes.set([0, 0, 0, 0], reqId.length + 1);
  assert.throws(
    () => byteProtocolCodecAdapter.decode(bytes),
    /Unknown wire discriminant/,
    'decode of id 250 should throw',
  );
}

// 5) truncated frame (no discriminant byte) must throw, not silently return.
{
  const truncated = str.enc('p:1'); // just the requestId, nothing after.
  assert.throws(
    () => byteProtocolCodecAdapter.decode(truncated),
    /missing discriminant byte/,
    'decode of truncated frame should throw',
  );
}

// 6) 32 KiB requestId exercises scanStrEnd mode-2 (4-byte compact-len prefix).
// Mirrors Rust `max_length_request_id_mode_two_round_trips` so both sides agree
// the high-shift branch in scanStrEnd is correct.
{
  const longId = 'y'.repeat(32 * 1024);
  const inner = new Uint8Array([0x00, 0xab, 0xcd]);
  const encoded = byteProtocolCodecAdapter.encode({
    requestId: longId,
    payload: { tag: 'host_account_get_request', value: inner },
  });
  // Confirm the encoder actually emitted a mode-2 prefix; otherwise the test
  // is silently exercising mode-0/1 and the assertion is hollow.
  assert.equal(encoded[0] & 0b11, 0b10, 'expected mode-2 compact-len prefix');
  const decoded = byteProtocolCodecAdapter.decode(encoded);
  assert.equal(decoded.requestId, longId);
  assert.equal(decoded.payload.tag, 'host_account_get_request');
  assert.equal(toHex(decoded.payload.value), toHex(inner));
}

console.log('all 6 wire-equality tests passed');
