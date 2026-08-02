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
  return (envelope) => {
    const decoded = decodeWireMessage(envelope.frame);
    if (decoded.isErr()) {
      sink({
        channelId: envelope.channelId,
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
      channelId: envelope.channelId,
      direction: envelope.dir,
      requestId,
      frameId: payload.id,
      role: "unknown",
      byteLength: payload.value.length,
      timestamp: Date.now(),
      ...(retainBytes ? { bytes: payload.value } : {}),
    };
    sink(frame);
  };
}
