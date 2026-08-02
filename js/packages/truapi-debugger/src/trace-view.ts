// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * The presentation model the drill-down renderer works in.
 *
 * A {@link WireTrace} is the *engine* contract: raw {@link ObservedFrame}s
 * grouped by `requestId`. The drill-down UI is mounted in two places with
 * structurally different taps behind them:
 *
 *  - the standalone app taps the raw wire (numeric `frameId`, `byteLength`,
 *    optional `bytes`), and resolves method names through the wire table;
 *  - dotli's panel taps the post-decode host-container bridge, so it has a
 *    method `tag` and a decoded payload but no wire discriminant or byte count.
 *
 * A single renderer over raw {@link WireTrace} would force one of those two to
 * fake the other's fields. {@link TraceView} is the honest shared surface: a
 * normalized per-op view both vantages can populate, with vantage-specific
 * fields left optional. {@link wireTraceToView} is the wire-vantage adapter;
 * dotli ships its own adapter over the same shape.
 *
 * The `WireTrace`/envelope/level-2 contract is unchanged by this module: it
 * only reads a `WireTrace` and produces a view.
 *
 * @module
 */

import type { FrameDirection, FrameRole } from "./observed-frame.js";
import type { WireMethodInfo, WireTrace } from "./wire-debugger.js";

/**
 * An op-level badge, surfaced against the whole trace in the drill-down header.
 *
 *  - `orphaned`: the trace has an opening frame with no matching close (a
 *    request with no response, a subscribe with no receive) or a close with no
 *    opening. Signals a dropped or still-in-flight op.
 *  - `malformed`: at least one frame failed to decode on the wire.
 *  - `retry-storm`: the op is one of a burst of like ops in a short window.
 *    This is a *cross-op* signal the single-trace renderer cannot see on its
 *    own, so it is supplied by the caller (the list/engine layer) rather than
 *    derived here. Left as a follow-up for the engine to compute.
 */
export type TraceBadge = "orphaned" | "malformed" | "retry-storm";

/** A per-frame badge, surfaced against a single row in the frame sequence. */
export type TraceFrameBadge = "malformed" | "orphaned";

/** One frame of an op, normalized for rendering. */
export interface TraceFrameView {
  /**
   * Stable index within the view. Used as the keyboard-navigation cursor and,
   * for level-2, the target a decode action addresses.
   */
  seq: number;
  /** Product-vantage direction: `out` left the product, `in` arrived at it. */
  direction: FrameDirection;
  /** Best-effort lifecycle role (request/response/receive/...). */
  role: FrameRole;
  /** Resolved dotted method, e.g. `account.getAccount`, when known. */
  method?: string;
  /** Wire discriminant, present on the raw-wire vantage. */
  frameId?: number;
  /** Encoded payload length in bytes, present when the vantage measures it. */
  byteLength?: number;
  /** Epoch ms the frame was observed. */
  timestamp: number;
  /** Offset in ms from the trace's first frame. */
  latencyFromStartMs: number;
  /**
   * Round-trip in ms from this frame back to the opening frame it answers,
   * present only on a closing frame that has a matched opener.
   */
  roundTripMs?: number;
  /** Badges for this frame alone. */
  badges: TraceFrameBadge[];
  /**
   * Whether a level-2 payload decode can even be attempted for this frame:
   * raw bytes were captured for it. Payload-blind by default regardless; this
   * only gates whether the affordance is *offered*.
   */
  decodable: boolean;
  /**
   * Whether this frame's method is on the sensitive denylist - its payload is
   * never decoded (only ever revealed via the explicit dev escape hatch). Drives
   * the privacy marker shown *before* any decode. Payload-blind: it reflects the
   * method id, nothing about the bytes.
   */
  sensitive?: boolean;
}

/** A whole op, normalized for the drill-down view. */
export interface TraceView {
  /** Correlation id shared by every frame in the op. */
  requestId: string;
  /**
   * Channel/host the op belongs to, when the vantage supplies it. `requestId`
   * is minted per-transport and is not unique across hosts dialing one debugger,
   * so the op list keys and filters on `(channelId, requestId)`.
   */
  channelId?: string;
  /** Epoch ms of the first frame. */
  startedAt: number;
  /** Epoch ms of the most recent frame. */
  lastAt: number;
  /** Total wall-clock span of the op in ms (`lastAt - startedAt`). */
  durationMs: number;
  /** Frames in arrival order. */
  frames: TraceFrameView[];
  /** Op-level badges. */
  badges: TraceBadge[];
  /** Whether any frame in the op is sensitive (drives the op-row privacy marker). */
  sensitive?: boolean;
}

/** Roles that open an op (expect a matching close later in the trace). */
const OPENING_ROLES: ReadonlySet<FrameRole> = new Set<FrameRole>([
  "request",
  "start",
]);

/** Roles that close or continue an op (expect a matching opener earlier). */
const CLOSING_ROLES: ReadonlySet<FrameRole> = new Set<FrameRole>([
  "response",
  "receive",
  "interrupt",
  "stop",
]);

/**
 * One frame described by a mount's adapter, before the view-level fields (`seq`,
 * latency, pairing, badges) are computed. The two vantages differ in what they
 * can fill: the wire vantage has `frameId`/`byteLength`/`bytes`; dotli's bridge
 * vantage has a `method` off the tag but no wire id or byte count. Everything
 * optional here is genuinely absent on one side, not merely unset.
 */
export interface TraceFrameInput {
  direction: FrameDirection;
  role: FrameRole;
  method?: string;
  frameId?: number;
  byteLength?: number;
  timestamp: number;
  /** Whether a level-2 decode can be attempted (raw bytes were retained). */
  decodable: boolean;
  /** Whether this frame's method is on the sensitive denylist. */
  sensitive?: boolean;
}

/** The raw shape a mount adapter hands to {@link buildTraceView}. */
export interface TraceViewInput {
  requestId: string;
  /** Channel/host the op belongs to, when the vantage supplies it. */
  channelId?: string;
  startedAt: number;
  lastAt: number;
  frames: readonly TraceFrameInput[];
  /**
   * Op-level signals the caller computes across traces (e.g. `retry-storm`).
   * Within-trace badges (`orphaned`, `malformed`) are derived here.
   */
  extraBadges?: readonly TraceBadge[];
}

/**
 * The vantage-agnostic core: assign each frame its `seq` and latency, pair
 * openers with closers to fill `roundTripMs` and flag orphans, then roll frame
 * badges up to the op. Both mount adapters ({@link wireTraceToView} and dotli's)
 * funnel through this so the frame sequence, latencies, and badges are computed
 * identically regardless of vantage.
 */
export function buildTraceView(input: TraceViewInput): TraceView {
  const frames: TraceFrameView[] = input.frames.map((frame, index) => ({
    seq: index,
    direction: frame.direction,
    role: frame.role,
    method: frame.method,
    frameId: frame.frameId,
    byteLength: frame.byteLength,
    timestamp: frame.timestamp,
    latencyFromStartMs: frame.timestamp - input.startedAt,
    badges: frame.role === "malformed" ? ["malformed"] : [],
    decodable: frame.decodable,
    sensitive: frame.sensitive ?? false,
  }));

  annotatePairing(frames);

  return {
    requestId: input.requestId,
    channelId: input.channelId,
    startedAt: input.startedAt,
    lastAt: input.lastAt,
    durationMs: input.lastAt - input.startedAt,
    frames,
    badges: deriveOpBadges(frames, input.extraBadges ?? []),
    sensitive: frames.some((f) => f.sensitive),
  };
}

/**
 * The op's method for display and filtering: the opening (request/start) frame's
 * method, else the first frame that resolves one. Shared so both terminal
 * frontends and the summary renderer agree on an op's name.
 */
export function viewMethod(view: TraceView): string {
  const opener =
    view.frames.find((f) => f.role === "request" || f.role === "start") ??
    view.frames.find((f) => f.method !== undefined);
  return opener?.method ?? "(unknown)";
}

/**
 * Adapt a raw-wire {@link WireTrace} into a {@link TraceView}. Method names are
 * resolved through the wire table (`frameId → method`); byte lengths and the
 * `decodable` flag come straight off the observed frames.
 */
export function wireTraceToView(
  trace: WireTrace,
  methodNames?: ReadonlyMap<number, WireMethodInfo>,
  extraBadges: readonly TraceBadge[] = [],
  sensitiveIds?: ReadonlySet<number>,
): TraceView {
  return buildTraceView({
    requestId: trace.requestId,
    channelId: trace.channelId,
    startedAt: trace.startedAt,
    lastAt: trace.lastAt,
    extraBadges,
    frames: trace.frames.map((frame): TraceFrameInput => {
      // The wire ingest leaves `role` as `"unknown"` (lifecycle is not on the
      // wire); the frameId's wire-table `kind` is the lifecycle role, so use it
      // when the frame has no better one. A `"malformed"` sentinel is kept.
      const info = methodNames?.get(frame.frameId);
      const role =
        frame.role === "unknown" && info !== undefined ? info.kind : frame.role;
      return {
        direction: frame.direction,
        role,
        method: info?.method,
        frameId: frame.frameId,
        byteLength: frame.byteLength,
        timestamp: frame.timestamp,
        decodable: frame.bytes !== undefined && frame.bytes.length > 0,
        sensitive: sensitiveIds?.has(frame.frameId) ?? false,
      };
    }),
  });
}

/**
 * Second pass over the frame sequence: pair openers with closers to fill in
 * `roundTripMs` and flag `orphaned` frames.
 *
 * All frames of a trace share one `requestId`, so pairing is positional: each
 * closing frame answers the most recent opener. An opener that never got any
 * close is orphaned (a request with no response, a subscribe that never
 * delivered); a closer with no opener before it is orphaned. An opener that got
 * at least one close is not orphaned even if it stays open - a live
 * subscription (start + receives, no stop yet) is healthy, not dropped.
 */
function annotatePairing(views: TraceFrameView[]): void {
  const openStack: number[] = [];
  const matched = new Set<number>();
  for (let i = 0; i < views.length; i++) {
    const view = views[i];
    if (OPENING_ROLES.has(view.role)) {
      openStack.push(i);
      continue;
    }
    if (CLOSING_ROLES.has(view.role)) {
      const openerIndex =
        openStack.length > 0 ? openStack[openStack.length - 1] : undefined;
      if (openerIndex === undefined) {
        markOrphan(view);
        continue;
      }
      view.roundTripMs = view.timestamp - views[openerIndex].timestamp;
      matched.add(openerIndex);
      // A `receive` keeps the subscription open for later receives; any other
      // close terminates the op and pops its opener.
      if (view.role !== "receive") {
        openStack.pop();
      }
    }
  }
  // Openers still open AND never answered are orphaned; a matched-but-open
  // opener (live subscription) is not.
  for (const openerIndex of openStack) {
    if (!matched.has(openerIndex)) {
      markOrphan(views[openerIndex]);
    }
  }
}

function markOrphan(view: TraceFrameView): void {
  if (!view.badges.includes("orphaned")) {
    view.badges.push("orphaned");
  }
}

/** Collapse per-frame badges plus caller-supplied signals into op-level badges. */
function deriveOpBadges(
  frames: readonly TraceFrameView[],
  extraBadges: readonly TraceBadge[],
): TraceBadge[] {
  const badges = new Set<TraceBadge>(extraBadges);
  for (const frame of frames) {
    if (frame.badges.includes("malformed")) {
      badges.add("malformed");
    }
    if (frame.badges.includes("orphaned")) {
      badges.add("orphaned");
    }
  }
  return [...badges];
}
