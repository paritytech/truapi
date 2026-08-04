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
 * Decode thread's {@link FrameValueDetail}, so a sensitive frame renders a
 * redacted state and never its value.
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
   * Decode results already resolved for this op, keyed by frame `seq`. The mount
   * fills this after a user acts on a frame (calling the Core session's
   * `frameDetail(requestId, seq)`) and re-renders. Frames absent from the map
   * show only their decode control, never a value.
   */
  decoded?: ReadonlyMap<number, FrameValueDetail>;
  /**
   * Offer the dev-only "reveal" affordance on *sensitive* frames (the escape
   * hatch). Off by default: a sensitive frame then shows its redacted state
   * upfront with no control. Only a mount whose session armed the reveal gate
   * sets this; the reveal itself is still an explicit, confirmed per-frame action.
   */
  offerReveal?: boolean;
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
  const offerReveal = options.offerReveal ?? false;
  const decoded = options.decoded;

  const header = renderHeader(view);
  const rows = view.frames
    .map((frame) =>
      renderFrameRow(frame, offerDecode, offerReveal, decoded?.get(frame.seq)),
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
  offerReveal: boolean,
  detail: FrameValueDetail | undefined,
): string {
  const glyph = DIRECTION_GLYPH[frame.direction];
  const method =
    frame.method === undefined
      ? `<span class="td-frame-method anon">id ${String(frame.frameId ?? "?")}</span>`
      : `<span class="td-frame-method">${esc(frame.method)}</span>`;
  const role = `<span class="td-frame-role td-role-${frame.role}">${esc(frame.role)}</span>`;
  // Privacy marker, shown before any decode: this frame carries material the
  // denylist keeps redacted. Reveals nothing the method name doesn't.
  const lock = frame.sensitive
    ? `<span class="td-frame-lock" title="sensitive method — payload redacted by default">🔒</span>`
    : "";
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
    lock +
    size +
    latency +
    (badges === "" ? "" : `<span class="td-frame-badges">${badges}</span>`) +
    `</div>`;

  const payload =
    offerDecode && frame.decodable
      ? `<div class="td-frame-payload">${renderDecodeBlock(frame, offerReveal, detail)}</div>`
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
 * The level-2 slot for one frame: a decode control plus, once resolved, the
 * decoded / redacted / bytes-only outcome. Rendered only when the mount offers
 * decode and the frame retained bytes.
 */
function renderDecodeBlock(
  frame: TraceFrameView,
  offerReveal: boolean,
  detail: FrameValueDetail | undefined,
): string {
  if (detail !== undefined) {
    return `<div class="td-frame-decoded">${renderFrameValueDetail(detail)}</div>`;
  }
  const size =
    frame.byteLength === undefined ? "" : ` · ${String(frame.byteLength)}B`;
  if (frame.sensitive) {
    // A sensitive frame stays redacted by default - so show that upfront rather
    // than a decode control that would only ever redact. When the dev reveal
    // gate is armed, offer a distinct, explicit reveal control instead (guarded
    // by a per-frame confirm on the client); it is NOT a `td-frame-decode-btn`,
    // so "Decode all" never sweeps it in.
    if (offerReveal) {
      return (
        `<button class="td-frame-reveal-btn" type="button" data-seq="${String(frame.seq)}" ` +
        `title="Reveal this sensitive payload (dev only) — asks for confirmation">` +
        `🔒 reveal sensitive${size}</button>`
      );
    }
    return `<div class="td-frame-decoded">${renderFrameValueDetail({
      kind: "redacted",
      reason: "sensitive method",
      byteLength: frame.byteLength ?? 0,
    })}</div>`;
  }
  // Non-sensitive pre-decode state: a blurred placeholder standing in for the
  // encoded payload. It carries NO real bytes - the renderer is payload-blind
  // and never sees them, so the blocks are decorative, sized only by byte
  // length. The button is the decode trigger; the value is fetched on demand.
  return (
    `<button class="td-frame-decode-btn" type="button" data-seq="${String(frame.seq)}" ` +
    `title="Decode this frame's payload (dev only)" aria-label="decode payload">` +
    `<span class="td-enc-blur" aria-hidden="true">${encodedGlyphs(frame.byteLength)}</span>` +
    `<span class="td-enc-hint">decode payload${size}</span>` +
    `</button>`
  );
}

/**
 * A capped run of block glyphs for the pre-decode blur: it conveys "an encoded
 * payload lives here" and roughly how large, without ever carrying the real
 * bytes. Purely decorative (aria-hidden); the byte length is the only input.
 */
function encodedGlyphs(byteLength: number | undefined): string {
  const n =
    byteLength === undefined
      ? 10
      : Math.max(8, Math.min(40, Math.ceil(byteLength / 2)));
  return "▓".repeat(n);
}

/**
 * Render a Core-thread {@link FrameValueDetail}. Shared by both mounts so the
 * redacted state is identical everywhere: a sensitive frame shows a clear
 * "redacted" label and its byte length, never its value.
 */
export function renderFrameValueDetail(detail: FrameValueDetail): string {
  switch (detail.kind) {
    case "redacted":
      return (
        `<div class="td-redacted" role="note">` +
        `<span class="td-redacted-tag">redacted</span> ` +
        `${esc(detail.reason)} · ${String(detail.byteLength)}B withheld` +
        `</div>`
      );
    case "bytes":
      return `<div class="td-bytes-only">${String(detail.byteLength)}B · payload not shown</div>`;
    case "decoded":
      // A revealed sensitive value is flagged so the mount can style it as the
      // danger it is (dev-only escape hatch); an ordinary decode is plain.
      return detail.sensitive === true
        ? `<pre class="td-detail-pre td-detail-danger" title="revealed sensitive material — dev only">${esc(stringifyValue(detail.value))}</pre>`
        : `<pre class="td-detail-pre">${esc(stringifyValue(detail.value))}</pre>`;
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
  // Op-row privacy marker + a filterable attribute: this op touches a method
  // whose payload stays redacted by default.
  const sensitiveAttr = view.sensitive ? ` data-sensitive="1"` : "";
  // Generation disambiguates ops that recycle a `(channelId, requestId)`; the
  // client keys rows and the drill-down on it so reused ids stay distinct.
  const genAttr = ` data-generation="${String(view.generation ?? 0)}"`;
  const lock = view.sensitive
    ? `<span class="td-op-lock" title="carries a sensitive method — payload redacted by default" aria-hidden="true">🔒</span>`
    : "";

  return (
    `<div class="td-op ${kindClass}${live ? " td-op-live" : ""}" ` +
    `data-request-id="${esc(view.requestId)}"${channelAttr}${genAttr}${sensitiveAttr} role="listitem" tabindex="-1">` +
    `<span class="td-op-kind" aria-hidden="true">${kindGlyph}</span>` +
    methodHtml +
    lock +
    (badges === "" ? "" : `<span class="td-op-badges">${badges}</span>`) +
    `<span class="td-op-meta">${meta}</span>` +
    `</div>`
  );
}
