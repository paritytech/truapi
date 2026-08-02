/**
 * The frame model the debugger works in.
 *
 * A host tap streams raw wire frames as `{ channelId, dir, frame: bytes }`
 * envelopes; {@link createDebugIngest} decodes each one into an
 * {@link ObservedFrame} - correlation id, wire discriminant, byte length, and
 * (dev-only) the raw bytes - which the trace and host engines consume. The core
 * never decodes; decoding happens here, in the debugger.
 *
 * @module
 */

/**
 * Direction of an observed wire frame relative to the product: `out` left the
 * product, `in` arrived at it.
 */
export type FrameDirection = "out" | "in";

/**
 * Role of an observed frame within the request/subscription lifecycle, derived
 * from its wire discriminant against the method's frame ids.
 */
export type FrameRole =
  | "request"
  | "response"
  | "start"
  | "stop"
  | "receive"
  | "interrupt"
  | "handshake"
  | "malformed"
  | "unknown";

/**
 * A single decoded wire frame. Carries the correlation `requestId`, the wire
 * discriminant, a best-effort lifecycle `role`, and the encoded byte length.
 * The raw `bytes` are present only when byte exposure is enabled - a dev-only
 * opt-in, since the raw wire can carry key material.
 */
export interface ObservedFrame {
  /**
   * Product channel the frame crossed, e.g. `"myapp.dot"`. Carried from the
   * host tap envelope. Because `requestId` is minted per transport (each host
   * mints `p:1`, `p:2`, …), it is unique only *within* a channel; grouping and
   * lookups key on `(channelId, requestId)` so two hosts' ops never merge.
   */
  channelId: string;
  /** Whether the frame was sent by the product (`out`) or received by it (`in`). */
  direction: FrameDirection;
  /** Correlation id shared by every frame of one request/subscription, within a channel. */
  requestId: string;
  /** Wire-table numeric discriminant of the frame's payload. */
  frameId: number;
  /** Best-effort lifecycle role inferred from the frame id. */
  role: FrameRole;
  /** Encoded SCALE payload length in bytes. */
  byteLength: number;
  /** Epoch ms at which the frame was observed. */
  timestamp: number;
  /** The raw SCALE payload bytes, present only when byte exposure is enabled. */
  bytes?: Uint8Array;
}

/**
 * Emit-only consumer of observed frames. The trace engine's
 * {@link WireDebugger.observe} is one; a host relay is another.
 */
export type TransportObserver = (frame: ObservedFrame) => void;
