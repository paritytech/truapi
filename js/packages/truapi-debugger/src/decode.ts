// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Level-2 decode: turn a frame's raw SCALE payload into a plain JS value, in the
 * drill-down detail path only, behind a dev-only opt-in.
 *
 * This is the one place the debugger looks *inside* a frame. Everything else -
 * the trace engine, `/traces`, the host tap - is payload-blind and stays that
 * way. The rules that make that safe live here:
 *
 *  - **Off by default.** With the decoder disabled every frame reports its byte
 *    length and nothing else; no payload is ever inspected.
 *  - **Reuse, don't reinvent.** Decoding is `WIRE_DECODE_TABLE[frameId]?.(bytes)`
 *    from `@parity/truapi/wire-decode` - the same generated, dev-only codecs the
 *    client uses. The debugger writes no codecs of its own.
 *  - **Sensitive denylist.** The generated table decodes *every* frame, including
 *    signing and login. The security of this feature is the denylist layered on
 *    top: a sensitive frame is never decoded, even with the toggle on - it
 *    reports its byte length labelled `"sensitive method"`. The denylist is
 *    itself generated: `SENSITIVE_FRAME_IDS` in `@parity/truapi/wire-table`
 *    carries every frame id of a method marked `#[wire(..., sensitive)]` on the
 *    Rust trait, so sensitivity is a property of the payload type, not a name
 *    the debugger pattern-matches.
 *
 * Nothing here is ever serialized into `/traces`; the detail it produces is
 * returned only from the explicit per-frame drill-down.
 *
 * @module
 */

import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";
import * as W from "@parity/truapi/wire-table";
import type { ObservedFrame } from "./observed-frame.js";

/**
 * Per-frame decode result for the drill-down detail path.
 *
 * `"bytes"` is the safe default returned whenever the decoder is off, the frame
 * carries no retained bytes, the id has no codec, or decoding throws.
 * `"redacted"` is returned for a sensitive frame even when the decoder is on.
 * `"decoded"` carries the plain JS value and is reachable only with the decoder
 * on, for a non-sensitive frame whose id is in the table.
 */
export type FrameValueDetail =
  | { kind: "decoded"; value: unknown; sensitive?: boolean }
  | { kind: "redacted"; reason: "sensitive method"; byteLength: number }
  | { kind: "bytes"; byteLength: number };

/**
 * The set of wire `frameId`s that must never be decoded, sourced directly from
 * the generated {@link W.SENSITIVE_FRAME_IDS}. That set is emitted by
 * `truapi-codegen` from every method marked `#[wire(..., sensitive)]` on the
 * Rust trait and carries all of the method's frame ids (request/response and
 * start/stop/interrupt/receive), so both legs of a sensitive op are redacted.
 *
 * Sensitivity therefore lives on the Rust payload type, not on a name the
 * debugger pattern-matches: a codegen rename cannot silently drop a family, and
 * a newly annotated method is denylisted the moment the client is regenerated.
 * The families it covers today:
 *
 *  - signing — every method (create-transaction(+legacy), sign-raw(+legacy),
 *    sign-payload(+legacy)): payloads to be signed and the resulting signatures.
 *  - account/statement-store proof creation: cryptographic proofs bound to a
 *    key/identity.
 *  - entropy/derive: key-derivation material.
 *  - account request-login / get-user-id: SSO/login and the user id it resolves.
 *  - local-storage read/write: a read response or a write request can carry
 *    tokens, session state, or PII. (`clear` carries only a key name and an
 *    empty response, so it is intentionally *not* sensitive.)
 *  - payment top-up: can carry a raw sr25519 secret key (PaymentTopUpSource).
 *  - coin-payment create-cheque/deposit/listen-for-payment: redeemable
 *    `encryptedSecrets` on a CoinPaymentCheque.
 *  - statement-store subscribe/submit: a SignedStatement's `decryptionKey`.
 *
 * Deliberately decodable, because they hold no key material: chain calls
 * (`CHAIN_*`) carry public on-chain data — headers, bodies, storage, runtime
 * calls, and the broadcast of already-public signed transactions — and are the
 * primary useful decode surface; chat, notifications, permissions, theme,
 * resource-allocation, and preimage likewise carry no credentials.
 *
 * Because sensitivity is a property of the payload *type*, the decoder also
 * applies a fail-closed content check (see {@link createFrameDecoder}) that
 * redacts any decoded value carrying a secret-named field — so a secret-bearing
 * method that was never annotated is still caught.
 */
export const SENSITIVE_FRAME_IDS: ReadonlySet<number> = W.SENSITIVE_FRAME_IDS;

/**
 * Field-name pattern for the fail-closed content check: keys whose name implies
 * key material or a bearer secret (`sr25519SecretKey`, `encryptedSecrets`,
 * `decryptionKey`, a mnemonic, a token/credential/passphrase, …). Deliberately
 * omits a bare `key` so public identifiers like `publicKey` still decode. This
 * is only a backstop — the authoritative guarantee is the generated
 * {@link SENSITIVE_FRAME_IDS} denylist (type-driven via `#[wire(sensitive)]`);
 * the content check catches any secret-bearing method that was never annotated.
 */
const SECRET_FIELD_RE =
  /secret|mnemonic|entropy|private|decrypt|token|credential|passphrase|password|apikey|bearer|seed/i;

/**
 * Does a decoded value carry a secret-named field anywhere in its structure?
 *
 * Sensitivity ultimately lives in the payload type, so this backs up
 * {@link SENSITIVE_FRAME_IDS}: a decoded value with a secret-named key is
 * redacted even if its method was not on the denylist. The `seen` set makes it
 * O(nodes) - each object is visited once - so it terminates in linear time on
 * cycles and shared-substructure DAGs, not just trees. Safe on arrays, tagged
 * unions, and nested structs.
 */
function containsSecretField(
  value: unknown,
  seen: WeakSet<object> = new WeakSet(),
  depth = 0,
): boolean {
  // Depth cap is generous headroom; the `seen` set is what bounds work, by
  // never revisiting an object even when the graph re-references it.
  if (depth > 64 || value === null || typeof value !== "object") return false;
  if (seen.has(value)) return false;
  seen.add(value);
  for (const [key, nested] of Object.entries(value as Record<string, unknown>)) {
    if (SECRET_FIELD_RE.test(key)) return true;
    if (containsSecretField(nested, seen, depth + 1)) return true;
  }
  return false;
}

/** Options for {@link createFrameDecoder}. */
export interface FrameDecoderOptions {
  /**
   * Master gate. `false` (the default) means the decoder never inspects a
   * payload: every frame reports bytes only. This is the dev-only opt-in.
   */
  enabled?: boolean;
  /**
   * Frame-id → decoder map. Defaults to the generated
   * {@link WIRE_DECODE_TABLE}; overridable for tests.
   */
  decodeTable?: Record<number, (payload: Uint8Array) => unknown>;
  /**
   * Frame ids that must never be decoded. Defaults to the generated
   * {@link SENSITIVE_FRAME_IDS} denylist.
   */
  sensitiveIds?: ReadonlySet<number>;
  /**
   * Second, independent gate that *allows* a sensitive frame to be decoded - but
   * only on an explicit per-frame `reveal` request (see {@link FrameDecoder.detail}),
   * never by default. Off by default and only meaningful when {@link enabled} is
   * also on. This is the dev-only "reveal sensitive" escape hatch: it is wired
   * from its own env gate (`TRUAPI_DEBUGGER_REVEAL_SENSITIVE`) so it is
   * structurally impossible to turn on in a shipped build, and even with it on
   * the safe default (redact) still holds until the operator confirms a reveal.
   */
  revealSensitive?: boolean;
}

/** Options for a single {@link FrameDecoder.detail} call. */
export interface FrameDetailOptions {
  /**
   * Explicit operator request to reveal a sensitive frame's value. Honored only
   * when the decoder was built with {@link FrameDecoderOptions.revealSensitive}
   * (and {@link FrameDecoderOptions.enabled}); otherwise ignored and the frame
   * redacts as usual. A reveal bypasses both the denylist and the content guard
   * for that one frame - it is the "show me everything" dev path.
   */
  reveal?: boolean;
}

/** A gated per-frame value decoder for the drill-down detail path. */
export interface FrameDecoder {
  /** Whether decoding is on. `false` ⇒ every `detail` is bytes-only. */
  readonly enabled: boolean;
  /** Whether the sensitive-reveal escape hatch is armed (still off by default per call). */
  readonly revealSensitive: boolean;
  /** The sensitive-frame denylist in effect (redacted unless explicitly revealed). */
  readonly sensitiveIds: ReadonlySet<number>;
  /** Resolve one frame to its {@link FrameValueDetail}. */
  detail(frame: ObservedFrame, options?: FrameDetailOptions): FrameValueDetail;
}

/**
 * Build a {@link FrameDecoder}. Off by default: pass `enabled: true` to opt in.
 * Even then, sensitive frames (see {@link SENSITIVE_FRAME_IDS}) are reported
 * as `"redacted"`, never decoded.
 */
export function createFrameDecoder(
  options: FrameDecoderOptions = {},
): FrameDecoder {
  const enabled = options.enabled ?? false;
  const revealSensitive = options.revealSensitive ?? false;
  const decodeTable = options.decodeTable ?? WIRE_DECODE_TABLE;
  const sensitiveIds = options.sensitiveIds ?? SENSITIVE_FRAME_IDS;

  const detail = (
    frame: ObservedFrame,
    detailOptions: FrameDetailOptions = {},
  ): FrameValueDetail => {
    if (!enabled) return { kind: "bytes", byteLength: frame.byteLength };
    // The reveal escape hatch fires only when the capability is armed AND the
    // operator explicitly asked for this frame. Absent either, the safe default
    // (redact sensitive / content-guard) stands - so the guarantee "sensitive
    // never decodes" holds by default even in a reveal-armed session.
    const reveal = revealSensitive && detailOptions.reveal === true;
    if (sensitiveIds.has(frame.frameId) && !reveal) {
      return {
        kind: "redacted",
        reason: "sensitive method",
        byteLength: frame.byteLength,
      };
    }
    const decode = decodeTable[frame.frameId];
    if (!decode || !frame.bytes) {
      return { kind: "bytes", byteLength: frame.byteLength };
    }
    try {
      const value = decode(frame.bytes);
      // Fail-closed net: redact if the decoded payload carries a secret-named
      // field, even though the method itself was not on the denylist - unless
      // this is an explicit reveal, which is the "show me everything" path.
      if (!reveal && containsSecretField(value)) {
        return {
          kind: "redacted",
          reason: "sensitive method",
          byteLength: frame.byteLength,
        };
      }
      // Mark a revealed value so the UI can style it as the danger it is.
      return reveal
        ? { kind: "decoded", value, sensitive: true }
        : { kind: "decoded", value };
    } catch {
      // A malformed or version-skewed payload must not break the drill-down;
      // fall back to the byte-length view.
      return { kind: "bytes", byteLength: frame.byteLength };
    }
  };

  return { enabled, revealSensitive, sensitiveIds, detail };
}
