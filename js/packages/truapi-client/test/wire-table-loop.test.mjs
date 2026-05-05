// Programmatic wire-equality loop.
//
// `wire-equality.test.mjs` exercises a handful of hand-picked tags. This file
// iterates every (id, tag) pair in `WIRE_TABLE` and asserts the codec
// round-trips a sentinel payload and produces the expected byte layout for
// each. Catches discriminant-specific bugs that a small fixed sample would
// miss: e.g. a stray off-by-one in `idForTag` for mid-table entries, or a
// `tag.length === 0`-shaped guard tripping for any specific tag.
//
// Mirrors the exhaustive-coverage check the Rust side gets for free via
// `wire_table_only_has_known_gaps` (which iterates 0..=max_id), but on the
// codec round-trip path rather than the lookup path.
//
// Run via: `bun run test`.

import assert from 'node:assert/strict';
import { byteProtocolCodecAdapter } from '../src/transport.ts';
import { WIRE_TABLE } from '../src/generated/wire-table.ts';
import { str } from '../src/scale.ts';

function toHex(u) {
  return Array.from(u).map((b) => b.toString(16).padStart(2, '0')).join('');
}

function expectedWire(reqId, tagId, valueBytes) {
  const idBytes = str.enc(reqId);
  const out = new Uint8Array(idBytes.length + 1 + valueBytes.length);
  out.set(idBytes, 0);
  out[idBytes.length] = tagId;
  out.set(valueBytes, idBytes.length + 1);
  return out;
}

assert.ok(WIRE_TABLE.length > 0, 'WIRE_TABLE must not be empty');

// Use a per-id sentinel payload so any cross-talk between ids surfaces as a
// concrete byte mismatch rather than a silent equality.
let checked = 0;
for (const [id, tag] of WIRE_TABLE) {
  const sentinel = new Uint8Array([id, 0xa5, ~id & 0xff, 0x5a]);
  const message = {
    requestId: `r:${id}`,
    payload: { tag, value: sentinel },
  };
  const encoded = byteProtocolCodecAdapter.encode(message);
  const expected = expectedWire(`r:${id}`, id, sentinel);
  assert.equal(
    toHex(encoded),
    toHex(expected),
    `encode mismatch for id=${id} tag=${tag}`,
  );

  const decoded = byteProtocolCodecAdapter.decode(encoded);
  assert.equal(decoded.requestId, `r:${id}`, `requestId mismatch for id=${id}`);
  assert.equal(decoded.payload.tag, tag, `tag mismatch for id=${id}`);
  assert.equal(
    toHex(decoded.payload.value),
    toHex(sentinel),
    `payload mismatch for id=${id} tag=${tag}`,
  );
  checked++;
}

console.log(`programmatic wire-table loop: ${checked} (id, tag) pairs round-tripped`);
