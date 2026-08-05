/**
 * Ingest: turn the host tap's wire envelopes into {@link ObservedFrame}s.
 *
 * The Rust host tap (`truapi-server`'s `DebugSink`) emits one envelope per
 * frame - `{ channelId, dir, frame: bytes }`, raw SCALE, opaque to the core.
 * The debugger decodes here: {@link decodeWireMessage} recovers the correlation
 * `requestId` and the wire discriminant, which is everything the trace engine
 * needs to group an op. This is the layer PG's design puts "in the debugger, not
 * the core".
 *
 * @module
 */

import { decodeWireMessage } from "@parity/truapi";
import type { ObservedFrame, TransportObserver } from "./observed-frame.js";
import type { WireMethodInfo } from "./wire-debugger.js";

/**
 * Version of the host→debugger wire envelope (`{ channelId, dir, frame }`).
 * Bumped when the envelope shape changes. Producers (the Rust `WsDebugSink`, the
 * web host's debugger link) stamp it alongside a codec identity so the debugger
 * can refuse to decode a frame against a wire contract that isn't its own -
 * frame ids are `u8` discriminants that get reassigned as the API evolves, so an
 * unversioned envelope from an older host would resolve to the wrong method and
 * the wrong value.
 */
export const WIRE_ENVELOPE_VERSION = 1;

/**
 * Default cap on retained `channelId` / `requestId` length. Shared so the
 * debugger server's channel registry clamps to the same bound as ingest and the
 * two keys stay equal (the UI filters by the clamped key).
 */
export const DEFAULT_MAX_ID_CHARS = 256;

/**
 * One wire frame as it crosses the host tap, matching the Rust
 * `DebugEvent::Frame { channel_id, dir, bytes }`. `frame` is the untouched
 * `ProtocolMessage` bytes; the debugger owns all decoding.
 */
export interface DebugFrameEnvelope {
  /** Product channel the frame belongs to, e.g. `"myapp.dot"`. */
  channelId: string;
  /**
   * Product-vantage: `out` left the product, `in` arrived at it. The Rust host
   * tap names directions host-vantage internally and flips to this convention
   * on the wire (`FrameDirection::wire_str`), so both ends agree here.
   */
  dir: "in" | "out";
  /** Raw SCALE `ProtocolMessage` bytes. */
  frame: Uint8Array;
}

/** Options for {@link createDebugIngest}. */
export interface DebugIngestOptions {
  /**
   * Retain each frame's raw SCALE bytes on the {@link ObservedFrame}. Off by
   * default: byte length is always recorded, but the bytes themselves are the
   * dev-only opt-in that level-2 decode needs. `/traces` never serializes them
   * either way; retaining them only makes the drill-down decoder able to run.
   */
  retainBytes?: boolean;
  /**
   * Reverse map from wire `frameId` to method info (build one with
   * {@link createMethodNameMap}). When set, each frame's lifecycle `role` is
   * resolved here from the frame id's wire-table `kind`, so *every* consumer -
   * the default console sink, the `forward` hook, and the trace engine - sees the
   * real role. Without it, `role` is left `"unknown"` and only the view adapter
   * recovers it.
   */
  methodNames?: ReadonlyMap<number, WireMethodInfo>;
  /**
   * Cap on retained `channelId` / `requestId` length. Anything able to reach the
   * host tap could otherwise send 200k-char ids, one copy per frame; real ids are
   * short (`myapp.dot`, `p:1`). Default 256.
   */
  maxIdChars?: number;
}

/**
 * Ingest that decodes each {@link DebugFrameEnvelope} and forwards the resulting
 * {@link ObservedFrame} to `sink` (typically a {@link WireDebugger}'s `observe`).
 *
 * `role` is left `"unknown"`: lifecycle roles (request/response/receive/…) are
 * derived from request/subscription correlation state, which lived in the client
 * transport and is not carried on the wire. Reconstructing it from the observed
 * request/response ordering is a follow-up; grouping by `requestId` does not need
 * it. An undecodable frame is surfaced as a `"malformed"` sentinel rather than
 * dropped, so the trace records the failure instead of going dark.
 *
 * Raw payload bytes are attached only when `retainBytes` is set - the dev-only
 * byte-exposure opt-in that the level-2 decoder consumes; otherwise a frame
 * carries its byte length and no payload.
 */
export function createDebugIngest(
  sink: TransportObserver,
  options: DebugIngestOptions = {},
): (envelope: DebugFrameEnvelope) => void {
  const retainBytes = options.retainBytes ?? false;
  const methodNames = options.methodNames;
  const maxIdChars = options.maxIdChars ?? DEFAULT_MAX_ID_CHARS;
  const clampId = (id: string): string =>
    id.length > maxIdChars ? id.slice(0, maxIdChars) : id;
  return (envelope) => {
    const channelId = clampId(envelope.channelId);
    const decoded = decodeWireMessage(envelope.frame);
    if (decoded.isErr()) {
      sink({
        channelId,
        direction: envelope.dir,
        requestId: "malformed",
        frameId: -1,
        role: "malformed",
        byteLength: envelope.frame.length,
        timestamp: Date.now(),
      });
      return;
    }
    const { requestId, payload } = decoded.value;
    const frame: ObservedFrame = {
      channelId,
      direction: envelope.dir,
      requestId: clampId(requestId),
      frameId: payload.id,
      // Resolve the lifecycle role from the frame id's wire-table kind (the same
      // kind wireTraceToView falls back to). Left "unknown" when no map is given
      // or the id is off-table.
      role: methodNames?.get(payload.id)?.kind ?? "unknown",
      byteLength: payload.value.length,
      timestamp: Date.now(),
      ...(retainBytes ? { bytes: payload.value } : {}),
    };
    sink(frame);
  };
}
