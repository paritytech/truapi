import { describe, expect, test } from "bun:test";

import * as W from "@parity/truapi/wire-table";
import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";

import { createFrameDecoder, type FrameValueDetail } from "./decode.js";
import type { ObservedFrame } from "./observed-frame.js";

/** A minimal observed frame for a given id/bytes; the fields decode ignores are stubbed. */
function frame(frameId: number, bytes?: Uint8Array): ObservedFrame {
  return {
    channelId: "myapp.dot",
    direction: "out",
    requestId: "p:1",
    frameId,
    role: "unknown",
    byteLength: bytes?.length ?? 0,
    timestamp: 0,
    ...(bytes ? { bytes } : {}),
  };
}

describe("frame decoder (real table) — decodes everything, no special-casing", () => {
  test("a non-sensitive frame decodes only with the toggle on", () => {
    // `connection-status.subscribe` start payload is `V1(void)` = a single 0x00
    // index byte: a real frame the generated table can decode.
    const id = W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start;
    const bytes = new Uint8Array([0]);

    const off = createFrameDecoder({ enabled: false });
    const offDetail = off.detail(frame(id, bytes));
    expect(offDetail.kind).toBe("bytes");
    if (offDetail.kind === "bytes") expect(offDetail.byteLength).toBe(1);

    const on = createFrameDecoder({ enabled: true });
    const onDetail = on.detail(frame(id, bytes));
    expect(onDetail.kind).toBe("decoded");
    // Sanity: the id really is in the generated decode table.
    expect(typeof WIRE_DECODE_TABLE[id]).toBe("function");
  });

  test("a formerly-'sensitive' signing frame decodes too (dev-only tool)", () => {
    // No denylist any more: a signing request decodes like every other frame.
    const decoder = createFrameDecoder({ enabled: true });
    const detail = decoder.detail(
      frame(W.SIGNING_SIGN_RAW.request, new Uint8Array([0])),
    );
    // It either decodes (id has a codec + valid bytes) or, on a codec throw for
    // the stub bytes, falls back to bytes — never a "redacted" state.
    expect(["decoded", "bytes"]).toContain(detail.kind);
    // Whatever the outcome, the kind is never the old "redacted" variant.
    expect(detail.kind).not.toBe("redacted");
  });

  test("disabled decoder is bytes-only for every frame", () => {
    const decoder = createFrameDecoder({ enabled: false });
    for (const id of [
      W.ACCOUNT_GET_ACCOUNT.request,
      W.SIGNING_SIGN_RAW.request,
      W.CHAIN_CALL_HEAD.request,
    ]) {
      expect(decoder.detail(frame(id, new Uint8Array([9]))).kind).toBe("bytes");
    }
  });
});

describe("frame decoder (injected table)", () => {
  const table = { 999: (b: Uint8Array) => ({ ok: Array.from(b) }) };

  test("decodes an id when enabled and bytes present", () => {
    const decoder = createFrameDecoder({ enabled: true, decodeTable: table });
    const detail = decoder.detail(frame(999, new Uint8Array([1, 2])));
    expect(detail).toEqual({
      kind: "decoded",
      value: { ok: [1, 2] },
    } satisfies FrameValueDetail);
  });

  test("decodes a secret-named field too — no content guard withholds it", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 999: () => ({ source: { sr25519SecretKey: "0xdead" } }) },
    });
    const detail = decoder.detail(frame(999, new Uint8Array([1])));
    expect(detail.kind).toBe("decoded");
    if (detail.kind === "decoded") {
      expect(detail.value).toEqual({ source: { sr25519SecretKey: "0xdead" } });
    }
  });

  test("falls back to bytes when the frame retained no bytes", () => {
    const decoder = createFrameDecoder({ enabled: true, decodeTable: table });
    expect(decoder.detail(frame(999)).kind).toBe("bytes");
  });

  test("falls back to bytes when the codec throws", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: {
        999: () => {
          throw new Error("bad payload");
        },
      },
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe("bytes");
  });

  test("falls back to bytes when the id has no codec", () => {
    const decoder = createFrameDecoder({ enabled: true, decodeTable: table });
    expect(decoder.detail(frame(1, new Uint8Array([1]))).kind).toBe("bytes");
  });
});
