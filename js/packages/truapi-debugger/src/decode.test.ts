import { describe, expect, test } from "bun:test";

import * as W from "@parity/truapi/wire-table";
import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";

import {
  createFrameDecoder,
  SENSITIVE_FRAME_IDS,
  type FrameValueDetail,
} from "./decode.js";
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

describe("sensitive denylist from the generated wire-table", () => {
  // Authoritative denylist: the generated SENSITIVE_FRAME_IDS set, emitted by
  // truapi-codegen from every `#[wire(..., sensitive)]` method on the Rust trait.
  const sensitive = SENSITIVE_FRAME_IDS;

  test("re-exports the generated SENSITIVE_FRAME_IDS set verbatim", () => {
    expect(sensitive).toBe(W.SENSITIVE_FRAME_IDS);
  });

  // Every id of each sensitive family must be present (both request/response,
  // both start/receive), so neither leg of a sensitive op can be decoded.
  const mustExclude: Record<string, ReadonlyArray<number>> = {
    "signing/create-transaction": Object.values(W.SIGNING_CREATE_TRANSACTION),
    "signing/sign-raw": Object.values(W.SIGNING_SIGN_RAW),
    "signing/sign-payload": Object.values(W.SIGNING_SIGN_PAYLOAD),
    "signing/sign-raw-legacy": Object.values(
      W.SIGNING_SIGN_RAW_WITH_LEGACY_ACCOUNT,
    ),
    "account/create-proof": Object.values(W.ACCOUNT_CREATE_ACCOUNT_PROOF),
    "statement-store/create-proof": Object.values(W.STATEMENT_STORE_CREATE_PROOF),
    "statement-store/create-proof-authorized": Object.values(
      W.STATEMENT_STORE_CREATE_PROOF_AUTHORIZED,
    ),
    "entropy/derive": Object.values(W.ENTROPY_DERIVE),
    "account/request-login": Object.values(W.ACCOUNT_REQUEST_LOGIN),
    "account/get-user-id": Object.values(W.ACCOUNT_GET_USER_ID),
    "account/sign-vrf": Object.values(W.ACCOUNT_SIGN_VRF),
    "local-storage/read": Object.values(W.LOCAL_STORAGE_READ),
    "local-storage/write": Object.values(W.LOCAL_STORAGE_WRITE),
    // Payment payloads carrying key material / bearer secrets (C1/M2).
    "payment/top-up": Object.values(W.PAYMENT_TOP_UP),
    "coin-payment/create-cheque": Object.values(W.COIN_PAYMENT_CREATE_CHEQUE),
    "coin-payment/deposit": Object.values(W.COIN_PAYMENT_DEPOSIT),
    "coin-payment/listen-for-payment": Object.values(
      W.COIN_PAYMENT_LISTEN_FOR_PAYMENT,
    ),
    // Statement-store subscribe/submit carry SignedStatement.decryptionKey.
    "statement-store/subscribe": Object.values(W.STATEMENT_STORE_SUBSCRIBE),
    "statement-store/submit": Object.values(W.STATEMENT_STORE_SUBMIT),
  };
  for (const [name, ids] of Object.entries(mustExclude)) {
    test(`excludes ${name}`, () => {
      for (const id of ids) expect(sensitive.has(id)).toBe(true);
    });
  }

  // Non-sensitive families stay decodable: chain reads, account reads, payments.
  // local-storage/clear is deliberately decodable — its request is just a key
  // name and its response is empty, so unlike read/write it carries no secret.
  const mustAllow: Record<string, ReadonlyArray<number>> = {
    "local-storage/clear": Object.values(W.LOCAL_STORAGE_CLEAR),
    "account/get-account": Object.values(W.ACCOUNT_GET_ACCOUNT),
    "account/connection-status": Object.values(
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
    ),
    "chain/call-head": Object.values(W.CHAIN_CALL_HEAD),
    "chain/broadcast-transaction": Object.values(W.CHAIN_BROADCAST_TRANSACTION),
    "payment/request": Object.values(W.PAYMENT_REQUEST),
  };
  for (const [name, ids] of Object.entries(mustAllow)) {
    test(`allows ${name}`, () => {
      for (const id of ids) expect(sensitive.has(id)).toBe(false);
    });
  }
});

describe("gated frame decoder (real table + denylist)", () => {
  test("a signing frame does NOT decode even with the toggle on", () => {
    const decoder = createFrameDecoder({ enabled: true });
    const detail = decoder.detail(
      frame(W.SIGNING_SIGN_RAW.request, new Uint8Array([1, 2, 3, 4])),
    );
    expect(detail.kind).toBe("redacted");
    if (detail.kind === "redacted") {
      expect(detail.reason).toBe("sensitive method");
      expect(detail.byteLength).toBe(4);
    }
  });

  test("every signing family id redacts, never decodes", () => {
    const decoder = createFrameDecoder({ enabled: true });
    for (const id of [
      ...Object.values(W.SIGNING_CREATE_TRANSACTION),
      ...Object.values(W.SIGNING_SIGN_PAYLOAD),
      ...Object.values(W.ACCOUNT_CREATE_ACCOUNT_PROOF),
      ...Object.values(W.ENTROPY_DERIVE),
      ...Object.values(W.ACCOUNT_REQUEST_LOGIN),
    ]) {
      const detail = decoder.detail(frame(id, new Uint8Array([0, 0])));
      expect(detail.kind).toBe("redacted");
    }
  });

  test("payment.topUp redacts (never decodes a raw private key) with toggle on (C1)", () => {
    const decoder = createFrameDecoder({ enabled: true });
    for (const id of Object.values(W.PAYMENT_TOP_UP)) {
      expect(decoder.detail(frame(id, new Uint8Array([0, 0]))).kind).toBe(
        "redacted",
      );
    }
  });

  test("a non-sensitive frame decodes only with the toggle on", () => {
    // `connection-status.subscribe` start payload is `V1(void)` = a single 0x00
    // index byte: a real, non-sensitive frame the generated table can decode.
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

describe("gated frame decoder (injected table for gating isolation)", () => {
  const table = { 999: (b: Uint8Array) => ({ ok: Array.from(b) }) };
  const sensitiveIds = new Set<number>([7]);

  test("decodes a non-sensitive id when enabled and bytes present", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: table,
      sensitiveIds,
    });
    const detail = decoder.detail(frame(999, new Uint8Array([1, 2])));
    expect(detail).toEqual({
      kind: "decoded",
      value: { ok: [1, 2] },
    } satisfies FrameValueDetail);
  });

  test("redacts a sensitive id before ever touching the table", () => {
    let called = false;
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 7: () => ((called = true), "leaked") },
      sensitiveIds,
    });
    const detail = decoder.detail(frame(7, new Uint8Array([1, 2, 3])));
    expect(detail.kind).toBe("redacted");
    expect(called).toBe(false);
  });

  test("falls back to bytes when the frame retained no bytes", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: table,
      sensitiveIds,
    });
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
      sensitiveIds,
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe("bytes");
  });

  test("content guard redacts a decoded value carrying a secret-named field", () => {
    // A non-denylisted id whose decoded payload nonetheless carries key material
    // (the C1/H1 class): the fail-closed content check must redact it.
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: {
        999: () => ({ source: { PrivateKey: { sr25519SecretKey: "0xdead" } } }),
      },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "redacted",
    );
  });

  test("content guard redacts encryptedSecrets (cheque bearer material)", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 999: () => ({ cheque: { encryptedSecrets: "0xbeef" } }) },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "redacted",
    );
  });

  test("content guard redacts a decryptionKey (statement key material)", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: {
        999: () => ({ statements: [{ decryptionKey: "0xc0ffee" }] }),
      },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "redacted",
    );
  });

  test("content guard redacts a generically-named credential field", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 999: () => ({ auth: { sessionToken: "0xabc" } }) },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "redacted",
    );
  });

  test("content guard still decodes a public identifier (publicKey)", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 999: () => ({ account: { publicKey: "0x01" } }) },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "decoded",
    );
  });

  test("content guard allows a benign value with no secret-named field", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: { 999: () => ({ account: { address: "0x01" }, amount: 5 }) },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "decoded",
    );
  });

  test("content guard terminates on a cyclic / shared-DAG value (no blowup)", () => {
    // The pre-visited-set guard hung on exactly this shape (a cycle with two
    // back-edges + shared substructure). If it regresses to exponential, this
    // test hangs instead of passing - which is the signal we want.
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: {
        999: () => {
          const a: Record<string, unknown> = {};
          const b: Record<string, unknown> = { a };
          a.b = b;
          a.self = a;
          return { a, b, both: [a, b, a, b] };
        },
      },
      sensitiveIds: new Set<number>(),
    });
    // Benign field names ⇒ decodes (and, crucially, returns promptly).
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "decoded",
    );
  });

  test("content guard still redacts a secret nested inside a cyclic value", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: {
        999: () => {
          const a: Record<string, unknown> = { secretKey: "0xdead" };
          const b: Record<string, unknown> = { a };
          a.b = b;
          return { a, b };
        },
      },
      sensitiveIds: new Set<number>(),
    });
    expect(decoder.detail(frame(999, new Uint8Array([1]))).kind).toBe(
      "redacted",
    );
  });
});

describe("sensitive reveal escape hatch (dev-only, safe by default)", () => {
  const table = { 7: (b: Uint8Array) => ({ secretKey: Array.from(b) }) };
  const sensitiveIds = new Set<number>([7]);

  test("with reveal capability OFF, an explicit reveal request is ignored", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      decodeTable: table,
      sensitiveIds,
      // revealSensitive omitted → off
    });
    const detail = decoder.detail(frame(7, new Uint8Array([1, 2])), {
      reveal: true,
    });
    expect(detail.kind).toBe("redacted");
  });

  test("with reveal capability ON but no explicit request, sensitive still redacts", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      revealSensitive: true,
      decodeTable: table,
      sensitiveIds,
    });
    // Default call (no reveal) — the safe default must still hold.
    expect(decoder.detail(frame(7, new Uint8Array([1, 2]))).kind).toBe(
      "redacted",
    );
  });

  test("with reveal capability ON and an explicit request, a sensitive frame decodes and is marked", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      revealSensitive: true,
      decodeTable: table,
      sensitiveIds,
    });
    const detail = decoder.detail(frame(7, new Uint8Array([1, 2])), {
      reveal: true,
    });
    expect(detail).toEqual({
      kind: "decoded",
      value: { secretKey: [1, 2] },
      sensitive: true,
    } satisfies FrameValueDetail);
  });

  test("an explicit reveal also bypasses the content guard for a non-denylisted frame", () => {
    const decoder = createFrameDecoder({
      enabled: true,
      revealSensitive: true,
      decodeTable: { 999: () => ({ auth: { sessionToken: "0xabc" } }) },
      sensitiveIds: new Set<number>(),
    });
    const detail = decoder.detail(frame(999, new Uint8Array([1])), {
      reveal: true,
    });
    expect(detail.kind).toBe("decoded");
    if (detail.kind === "decoded") expect(detail.sensitive).toBe(true);
  });

  test("the master gate still wins: reveal armed but decode disabled ⇒ bytes only", () => {
    const decoder = createFrameDecoder({
      enabled: false,
      revealSensitive: true,
      decodeTable: table,
      sensitiveIds,
    });
    expect(decoder.detail(frame(7, new Uint8Array([1, 2])), { reveal: true }).kind).toBe(
      "bytes",
    );
  });
});
