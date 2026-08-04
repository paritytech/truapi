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
   * Turn on level-2 value decode in the drill-down detail path. Off by default.
   * When on, the session retains raw frame bytes so {@link DebugSession.frameDetail}
   * can decode non-sensitive frames; `/traces` stays payload-blind regardless
   * (it never reads bytes or decoded values), and sensitive frames are never
   * decoded even here. When off, `frameDetail` reports byte length only.
   */
  decodeValues?: boolean;
  /**
   * Arm the dev-only sensitive-reveal escape hatch. Off by default and only
   * meaningful when {@link decodeValues} is also on. Even armed, a sensitive
   * frame still redacts unless {@link DebugSession.frameDetail} is called with an
   * explicit `reveal` (the operator confirms per frame). Wired from
   * `TRUAPI_DEBUGGER_REVEAL_SENSITIVE`, so it cannot be set in a shipped build.
   */
  revealSensitive?: boolean;
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
  /** Whether the dev-only sensitive-reveal escape hatch is armed for this session. */
  readonly revealSensitive: boolean;
  /**
   * Frame ids that are never decoded (the sensitive denylist). Exposed so a view
   * can mark a frame/op as carrying redacted material *before* any decode - the
   * marker is payload-blind (it reveals nothing the method name doesn't) and
   * holds regardless of {@link DebugSessionOptions.decodeValues}.
   */
  readonly sensitiveIds: ReadonlySet<number>;
  /**
   * Drill-down: resolve one frame (by its trace `requestId` and index within
   * that trace) to a {@link FrameValueDetail}. Pass `channelId` to disambiguate
   * when more than one host is connected (each mints the same `p:N` ids).
   * Returns `undefined` if no such frame exists. This is the *only* path that can
   * surface a decoded value, and only when {@link DebugSessionOptions.decodeValues}
   * is on and the frame is not sensitive; otherwise it reports byte length only.
   */
  frameDetail(
    requestId: string,
    index: number,
    channelId?: string,
    reveal?: boolean,
    generation?: number,
  ): FrameValueDetail | undefined;
}

/**
 * Build a {@link DebugSession}. The `frameId → method` map is derived from the
 * generated wire table and client service names, so traces show
 * `account.getAccount` rather than a bare `id=22`.
 */
export function createDebugSession(
  options: DebugSessionOptions = {},
): DebugSession {
  const decodeValues = options.decodeValues ?? false;
  // Reveal is meaningless without decode; fold the master gate in so the
  // reported capability can never claim more than the session can actually do.
  const revealSensitive = decodeValues && (options.revealSensitive ?? false);
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
  const decoder = createFrameDecoder({
    enabled: decodeValues,
    revealSensitive,
  });

  const frameDetail = (
    requestId: string,
    index: number,
    channelId?: string,
    reveal?: boolean,
    generation?: number,
  ): FrameValueDetail | undefined => {
    const frame = wireDebugger.trace(requestId, channelId, generation)?.frames[
      index
    ];
    return frame ? decoder.detail(frame, { reveal }) : undefined;
  };

  return {
    handleEnvelope,
    traceEngine: wireDebugger,
    methodNames,
    decodeValues,
    revealSensitive: decoder.revealSensitive,
    sensitiveIds: decoder.sensitiveIds,
    frameDetail,
  };
}
