// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Level-2 decode: turn a frame's raw SCALE payload into a plain JS value, in the
 * drill-down detail path.
 *
 * This is the one place the debugger looks *inside* a frame. Everything else -
 * the trace engine, `/traces`, the host tap - is payload-blind and stays that
 * way. The rules that make that work live here:
 *
 *  - **Dev-only tool: decode everything.** This debugger decodes every frame it
 *    can, with no "sensitive" special-casing. A developer inspecting their own
 *    session's traffic sees the real values. When decoding is disabled every
 *    frame reports its byte length only.
 *  - **Reuse, don't reinvent.** Decoding is `WIRE_DECODE_TABLE[frameId]?.(bytes)`
 *    from `@parity/truapi/wire-decode` - the same generated, dev-only codecs the
 *    client uses. The debugger writes no codecs of its own.
 *
 * Nothing here is ever serialized into `/traces`; the detail it produces is
 * returned only from the explicit per-frame drill-down.
 *
 * @module
 */

import { WIRE_DECODE_TABLE } from "@parity/truapi/wire-decode";
import type { ObservedFrame } from "./observed-frame.js";

/**
 * Per-frame decode result for the drill-down detail path.
 *
 * `"decoded"` carries the plain JS value, returned whenever the decoder is on
 * and the frame's id has a codec that decodes its retained bytes. `"bytes"` is
 * the fallback: the decoder is off, the frame carries no retained bytes, its id
 * has no codec, or decoding threw.
 */
export type FrameValueDetail =
  | { kind: "decoded"; value: unknown }
  | { kind: "bytes"; byteLength: number };

/** Options for {@link createFrameDecoder}. */
export interface FrameDecoderOptions {
  /**
   * Master gate. `false` (the default) means the decoder never inspects a
   * payload: every frame reports bytes only.
   */
  enabled?: boolean;
  /**
   * Frame-id → decoder map. Defaults to the generated
   * {@link WIRE_DECODE_TABLE}; overridable for tests.
   */
  decodeTable?: Record<number, (payload: Uint8Array) => unknown>;
}

/** A gated per-frame value decoder for the drill-down detail path. */
export interface FrameDecoder {
  /** Whether decoding is on. `false` ⇒ every `detail` is bytes-only. */
  readonly enabled: boolean;
  /** Resolve one frame to its {@link FrameValueDetail}. */
  detail(frame: ObservedFrame): FrameValueDetail;
}

/**
 * Build a {@link FrameDecoder}. Off by default: pass `enabled: true` to opt in.
 * When on, every frame with a codec and retained bytes decodes to its value.
 */
export function createFrameDecoder(
  options: FrameDecoderOptions = {},
): FrameDecoder {
  const enabled = options.enabled ?? false;
  const decodeTable = options.decodeTable ?? WIRE_DECODE_TABLE;

  const detail = (frame: ObservedFrame): FrameValueDetail => {
    if (!enabled) return { kind: "bytes", byteLength: frame.byteLength };
    const decode = decodeTable[frame.frameId];
    if (!decode || !frame.bytes) {
      return { kind: "bytes", byteLength: frame.byteLength };
    }
    try {
      return { kind: "decoded", value: decode(frame.bytes) };
    } catch {
      // A malformed or version-skewed payload must not break the drill-down;
      // fall back to the byte-length view.
      return { kind: "bytes", byteLength: frame.byteLength };
    }
  };

  return { enabled, detail };
}
