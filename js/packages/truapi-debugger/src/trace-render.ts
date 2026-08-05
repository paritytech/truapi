// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * The one drill-down renderer, mounted in both the standalone app and dotli's
 * panel.
 *
 * "One level deeper": given a selected op, render its frame sequence -
 * request→response, or subscribe→receive×N→stop - with method, direction, byte
 * length, latency, and orphaned/malformed/retry-storm badges. It is a pure
 * `TraceView → HTML` function so the two mounts render identically; each mount
 * supplies the {@link TraceView} through its own adapter (see {@link
 * wireTraceToView} for the wire vantage).
 *
 * Payload-blind by default. Level-2 value decode is offered only when a mount
 * opts in (`offerDecode`) and passes decode results back in (`decoded`); the
 * renderer never touches bytes itself. Decode results come from the Core +
 * Decode thread's {@link FrameValueDetail}: a frame renders either its decoded
 * value or its byte length.
 *
 * The renderer emits HTML strings (both mounts assign `innerHTML`) using `td-*`
 * classes so one stylesheet covers both. Every interpolated string that came
 * off the wire (`requestId`, `method`) is escaped.
 *
 * @module
 */

import type { FrameValueDetail } from "./decode.js";
import type {
  TraceBadge,
  TraceFrameBadge,
  TraceFrameView,
  TraceView,
} from "./trace-view.js";

/** Options controlling a single drill-down render. */
export interface RenderTraceDetailOptions {
  /**
   * Offer the per-frame level-2 decode affordance for decodable frames. Off by
   * default: the view stays payload-blind and shows no decode control.
   */
  offerDecode?: boolean;
  /**
   * Decoded values for this op, keyed by frame `seq`. A dev-only mount decodes
   * every frame up front (calling the Core session's `frameDetail`) and passes
   * the results here. A frame absent from the map falls back to its byte length.
   */
  decoded?: ReadonlyMap<number, FrameValueDetail>;
}

/** HTML-escape a wire-sourced string before it touches `innerHTML`. */
function esc(value: string): string {
  return value.replace(/[&<>"']/g, (c) => {
    switch (c) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case '"':
        return "&quot;";
      default:
        return "&#39;";
    }
  });
}

/** `1234` → `1.23s`, `42` → `42ms`, for compact latency display. */
function formatMs(ms: number): string {
  if (ms < 1000) return `${String(Math.round(ms))}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

const DIRECTION_GLYPH: Record<TraceFrameView["direction"], string> = {
  out: "▶",
  in: "◀",
};

/**
 * Render the drill-down detail for one op. Returns an HTML fragment for a
 * mount's detail pane (`.td-detail` in dotli, the detail column in the app).
 */
export function renderTraceDetail(
  view: TraceView,
  options: RenderTraceDetailOptions = {},
): string {
  const offerDecode = options.offerDecode ?? false;
  const decoded = options.decoded;

  const header = renderHeader(view);
  const rows = view.frames
    .map((frame) =>
      renderFrameRow(frame, offerDecode, decoded?.get(frame.seq)),
    )
    .join("");

  return (
    `<div class="td-trace" data-request-id="${esc(view.requestId)}">` +
    header +
    `<div class="td-frames" role="list">${rows}</div>` +
    `</div>`
  );
}

function renderHeader(view: TraceView): string {
  const badges = view.badges.map(renderOpBadge).join("");
  const frameCount = view.frames.length;
  return (
    `<div class="td-trace-head">` +
    `<code class="td-trace-id">${esc(view.requestId)}</code>` +
    `<span class="td-trace-meta">${String(frameCount)} frame${frameCount === 1 ? "" : "s"} · ${formatMs(view.durationMs)}</span>` +
    (badges === "" ? "" : `<span class="td-trace-badges">${badges}</span>`) +
    `</div>`
  );
}

const OP_BADGE_LABEL: Record<TraceBadge, string> = {
  orphaned: "orphaned",
  malformed: "malformed",
  "retry-storm": "retry storm",
  truncated: "truncated",
};

function renderOpBadge(badge: TraceBadge): string {
  return `<span class="td-badge td-badge-${badge}" title="${esc(badgeTitle(badge))}">${esc(OP_BADGE_LABEL[badge])}</span>`;
}

function badgeTitle(badge: TraceBadge): string {
  switch (badge) {
    case "orphaned":
      return "An opening frame has no matching close, or a close has no opener";
    case "malformed":
      return "A frame failed to decode on the wire";
    case "retry-storm":
      return "This op is one of a burst of like ops in a short window";
    case "truncated":
      return "Older frames were dropped to stay under the frame/byte cap";
  }
}

const FRAME_BADGE_LABEL: Record<TraceFrameBadge, string> = {
  malformed: "malformed",
  orphaned: "orphaned",
};

function renderFrameRow(
  frame: TraceFrameView,
  offerDecode: boolean,
  detail: FrameValueDetail | undefined,
): string {
  const glyph = DIRECTION_GLYPH[frame.direction];
  const method =
    frame.method === undefined
      ? `<span class="td-frame-method anon">id ${String(frame.frameId ?? "?")}</span>`
      : `<span class="td-frame-method">${esc(frame.method)}</span>`;
  const role = `<span class="td-frame-role td-role-${frame.role}">${esc(frame.role)}</span>`;
  const size =
    frame.byteLength === undefined
      ? ""
      : `<span class="td-frame-bytes">${String(frame.byteLength)}B</span>`;
  const latency = renderLatency(frame);
  const badges = frame.badges
    .map(
      (b) =>
        `<span class="td-frame-badge td-badge-${b}">${esc(FRAME_BADGE_LABEL[b])}</span>`,
    )
    .join("");

  // The frame's meta (direction, role, method, size, latency, badges) is one
  // grouped cell so a mount can pin the level-2 payload into a fixed second
  // column beside it - every frame's decoded box then opens in the same aligned
  // space rather than trailing variable-width meta.
  const meta =
    `<div class="td-frame-meta">` +
    `<span class="td-frame-dir td-dir-${frame.direction}">${glyph}</span>` +
    role +
    method +
    size +
    latency +
    (badges === "" ? "" : `<span class="td-frame-badges">${badges}</span>`) +
    `</div>`;

  const payload =
    offerDecode && frame.decodable
      ? `<div class="td-frame-payload">${renderDecodeBlock(frame, detail)}</div>`
      : "";

  return (
    `<div class="td-frame" data-seq="${String(frame.seq)}" role="listitem">` +
    meta +
    payload +
    `</div>`
  );
}

function renderLatency(frame: TraceFrameView): string {
  // A closing frame that answers an opener shows its round-trip; everything
  // else shows its offset from the op's first frame.
  if (frame.roundTripMs !== undefined) {
    return `<span class="td-frame-latency" title="round trip to the frame it answers — debugger-observed, includes transport/queueing delay">⟳ ${formatMs(frame.roundTripMs)}</span>`;
  }
  if (frame.latencyFromStartMs === 0) {
    return `<span class="td-frame-latency td-latency-start">+0</span>`;
  }
  return `<span class="td-frame-latency" title="offset from the op's first frame — debugger-observed, includes transport/queueing delay">+${formatMs(frame.latencyFromStartMs)}</span>`;
}

/**
 * The level-2 payload slot for one frame. A dev-only tool decodes every frame,
 * so this shows the decoded value; a frame whose value could not be resolved
 * (bytes not retained, or a decode miss) shows its byte length instead.
 */
function renderDecodeBlock(
  frame: TraceFrameView,
  detail: FrameValueDetail | undefined,
): string {
  if (detail !== undefined) {
    return `<div class="td-frame-decoded">${renderFrameValueDetail(detail)}</div>`;
  }
  const size =
    frame.byteLength === undefined ? "" : `${String(frame.byteLength)}B · `;
  return `<div class="td-bytes-only">${size}payload not shown</div>`;
}

/**
 * Render a Core-thread {@link FrameValueDetail}. Shared by both mounts so the
 * outcome is identical everywhere: a frame shows its decoded value, or its byte
 * length when no value is available.
 */
export function renderFrameValueDetail(detail: FrameValueDetail): string {
  switch (detail.kind) {
    case "bytes":
      return `<div class="td-bytes-only">${String(detail.byteLength)}B · payload not shown</div>`;
    case "decoded":
      return `<pre class="td-detail-pre">${esc(stringifyValue(detail.value))}</pre>`;
  }
}

/** Pretty-print a decoded value for a `<pre>`, tolerating cyclic/bigint inputs. */
function stringifyValue(value: unknown): string {
  try {
    return JSON.stringify(
      value,
      (_key, v: unknown) => (typeof v === "bigint" ? `${v.toString()}n` : v),
      2,
    );
  } catch {
    return String(value);
  }
}

/** Roles that mark an op as a subscription rather than a request/response. */
const SUBSCRIPTION_ROLES: ReadonlySet<TraceFrameView["role"]> = new Set([
  "start",
  "receive",
  "stop",
  "interrupt",
]);

/** The op's method: the first opening frame's method, else the first known one. */
function operationMethod(view: TraceView): string | undefined {
  const opener = view.frames.find(
    (f) => f.role === "request" || f.role === "start",
  );
  if (opener?.method !== undefined) {
    return opener.method;
  }
  return view.frames.find((f) => f.method !== undefined)?.method;
}

/** Whether the op is a subscription (has a start/receive/stop/interrupt frame). */
function isSubscription(view: TraceView): boolean {
  return view.frames.some((f) => SUBSCRIPTION_ROLES.has(f.role));
}

/**
 * Render one operation-list row: the primary view's unit, one per op. Shows the
 * method, a request/subscription glyph, op-level badges, frame count, and
 * duration. A subscription with no `stop` frame is marked live.
 *
 * Pure and stateless: the mount toggles `.selected` and manages the keyed diff.
 * `data-request-id` (+ `data-channel-id` when known) identify the row for
 * selection and channel filtering. Payload-blind: only shape and timing here.
 */
export function renderOperationRow(view: TraceView): string {
  const method = operationMethod(view);
  const sub = isSubscription(view);
  const live = sub && !view.frames.some((f) => f.role === "stop");
  const kindGlyph = sub ? "⟳" : "▶";
  const kindClass = sub ? "td-op-sub" : "td-op-req";

  const methodHtml =
    method === undefined
      ? `<span class="td-op-method anon">(unknown)</span>`
      : `<span class="td-op-method" title="${esc(method)}">${esc(method)}</span>`;
  const badges = view.badges.map(renderOpBadge).join("");
  const count = view.frames.length;
  const meta =
    `${String(count)} frame${count === 1 ? "" : "s"} · ` +
    (live ? `live · ${formatMs(view.durationMs)}` : formatMs(view.durationMs));

  const channelAttr =
    view.channelId === undefined
      ? ""
      : ` data-channel-id="${esc(view.channelId)}"`;
  // Generation disambiguates ops that recycle a `(channelId, requestId)`; the
  // client keys rows and the drill-down on it so reused ids stay distinct.
  const genAttr = ` data-generation="${String(view.generation ?? 0)}"`;

  return (
    `<div class="td-op ${kindClass}${live ? " td-op-live" : ""}" ` +
    `data-request-id="${esc(view.requestId)}"${channelAttr}${genAttr} role="listitem" tabindex="-1">` +
    `<span class="td-op-kind" aria-hidden="true">${kindGlyph}</span>` +
    methodHtml +
    (badges === "" ? "" : `<span class="td-op-badges">${badges}</span>`) +
    `<span class="td-op-meta">${meta}</span>` +
    `</div>`
  );
}
