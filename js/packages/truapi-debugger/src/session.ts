/**
 * A debug session: the trace engine wired to the ingest.
 *
 * A host dials the debugger and streams {@link DebugFrameEnvelope}s over a
 * socket; each is handed to {@link DebugSession.handleEnvelope}, decoded, and
 * grouped into per-`requestId` traces readable via {@link DebugSession.traces}.
 *
 * The socket itself is deliberately not here. The debugger app is a WS server
 * (hosts dial outward to it), but binding the socket is a thin edge: accept a
 * connection, JSON/CBOR-decode each message into a {@link DebugFrameEnvelope},
 * and call `handleEnvelope`. Keeping that edge out of this module lets the
 * session compile and unit-test without a socket transport or Node types.
 *
 * @module
 */

import {
  createWireDebugger,
  createMethodNameMap,
  type WireDebugger,
  type WireMethodInfo,
} from "./wire-debugger.js";
import { createDebugIngest, type DebugFrameEnvelope } from "./ingest.js";
import { createFrameDecoder, type FrameValueDetail } from "./decode.js";
import type { TraceView } from "./trace-view.js";
import * as W from "@parity/truapi/wire-table";
import { createClient, createTransport } from "@parity/truapi";

/** A provider that sends and receives nothing; used only to enumerate service names. */
const NOOP_PROVIDER = {
  postMessage() {},
  subscribe() {
    return () => {};
  },
  dispose() {},
};

/** Options for {@link createDebugSession}. */
export interface DebugSessionOptions {
  /**
   * Turn on level-2 value decode in the drill-down detail path. On by default
   * (this is a dev-only tool that decodes everything). When on, the session
   * retains raw frame bytes so {@link DebugSession.frameDetail} can decode a
   * frame; `/traces` stays payload-blind regardless (it never reads bytes or
   * decoded values). When off, `frameDetail` reports byte length only.
   */
  decodeValues?: boolean;
}

/** Live debug session: feed it envelopes, read back grouped traces. */
export interface DebugSession {
  /** Handle one wire envelope from the host tap. */
  handleEnvelope(envelope: DebugFrameEnvelope): void;
  /** The underlying trace engine (traces, per-id lookup, clear). */
  readonly traceEngine: WireDebugger;
  /** Reverse map from wire `frameId` to method, for labelling frames in a view. */
  readonly methodNames: ReadonlyMap<number, WireMethodInfo>;
  /** Whether level-2 value decode is enabled for this session. */
  readonly decodeValues: boolean;
  /**
   * Drill-down: resolve one frame (by its trace `requestId` and index within
   * that trace) to a {@link FrameValueDetail}. Pass `channelId` to disambiguate
   * when more than one host is connected (each mints the same `p:N` ids).
   * Returns `undefined` if no such frame exists. This is the *only* path that can
   * surface a decoded value, and only when {@link DebugSessionOptions.decodeValues}
   * is on; otherwise it reports byte length only.
   */
  frameDetail(
    requestId: string,
    index: number,
    channelId?: string,
    generation?: number,
  ): FrameValueDetail | undefined;
  /**
   * Decode every frame of one op in a single trace resolution, keyed by frame
   * index (`seq`). This is the batch path the inline drill-down uses, so a mount
   * resolves the op once rather than re-resolving it per frame. Empty when decode
   * is off or the op is not found.
   */
  decodedFrames(
    requestId: string,
    channelId?: string,
    generation?: number,
  ): Map<number, FrameValueDetail>;
}

/**
 * Build a {@link DebugSession}. The `frameId → method` map is derived from the
 * generated wire table and client service names, so traces show
 * `account.getAccount` rather than a bare `id=22`.
 */
export function createDebugSession(
  options: DebugSessionOptions = {},
): DebugSession {
  // Dev-only tool: decode everything by default. The developer is looking at
  // their own session's traffic, so value decode is ON unless a caller explicitly
  // turns it off (tests do).
  const decodeValues = options.decodeValues ?? true;
  const serviceNames = Object.keys(createClient(createTransport(NOOP_PROVIDER)));
  const methodNames = createMethodNameMap(
    W as unknown as Record<string, unknown>,
    serviceNames,
  );
  // No `sink`: a session accumulates traces for the view/`/traces`; it must not
  // spam the server console with a line per frame (the sink default is
  // `console.debug`). Consumers read `traceEngine`, not stdout.
  const wireDebugger = createWireDebugger({ methodNames, sink: () => {} });
  // Raw bytes are retained only when decode is on - they exist solely to feed
  // the drill-down decoder, and `/traces` never serializes them. `methodNames`
  // resolves each frame's role at ingest, so the engine and any forward hook see
  // the real role rather than "unknown".
  const handleEnvelope = createDebugIngest(wireDebugger.observe, {
    retainBytes: decodeValues,
    methodNames,
  });
  const decoder = createFrameDecoder({ enabled: decodeValues });

  const frameDetail = (
    requestId: string,
    index: number,
    channelId?: string,
    generation?: number,
  ): FrameValueDetail | undefined => {
    const frame = wireDebugger.trace(requestId, channelId, generation)?.frames[
      index
    ];
    return frame ? decoder.detail(frame) : undefined;
  };

  const decodedFrames = (
    requestId: string,
    channelId?: string,
    generation?: number,
  ): Map<number, FrameValueDetail> => {
    const decoded = new Map<number, FrameValueDetail>();
    if (!decodeValues) return decoded;
    // Resolve the op once, then decode each frame off the resolved trace, rather
    // than re-resolving (a linear scan over every retained trace) per frame.
    const trace = wireDebugger.trace(requestId, channelId, generation);
    if (!trace) return decoded;
    trace.frames.forEach((frame, index) => {
      const detail = decoder.detail(frame);
      if (detail !== undefined) decoded.set(index, detail);
    });
    return decoded;
  };

  return {
    handleEnvelope,
    traceEngine: wireDebugger,
    methodNames,
    decodeValues,
    frameDetail,
    decodedFrames,
  };
}

/**
 * Decode every frame of an op up front, keyed by frame `seq`, ready to hand to
 * {@link renderTraceDetail}'s `decoded` option. A dev-only tool shows values
 * inline rather than behind a per-frame control, so a mount decodes the whole
 * op in one pass. Returns an empty map when the session has decode off.
 */
export function decodeTraceFrames(
  session: DebugSession,
  view: TraceView,
): Map<number, FrameValueDetail> {
  return session.decodedFrames(view.requestId, view.channelId, view.generation);
}
