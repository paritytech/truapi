// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Terminal renderer for the drill-down, the text counterpart of the HTML
 * {@link renderTraceDetail}. Same {@link TraceView} input, so the terminal
 * viewer ({@link module:cli}) and the web inspector show the same ops, badges,
 * redaction, and decoded values off one engine - no forked formatter, no forked
 * denylist. Pure `TraceView → string`; the CLI supplies the view and the decode
 * results, exactly as the web mount does.
 *
 * @module
 */

import type { FrameValueDetail } from "./decode.js";
import { viewMethod, type TraceFrameView, type TraceView } from "./trace-view.js";

// Color only on an interactive terminal, and never when NO_COLOR is set.
const USE_COLOR =
  process.env.NO_COLOR === undefined && process.stdout.isTTY === true;

function paint(code: string, s: string): string {
  return USE_COLOR ? `\x1b[${code}m${s}\x1b[0m` : s;
}
const bold = (s: string): string => paint("1", s);
const dim = (s: string): string => paint("2", s);
const red = (s: string): string => paint("31", s);
const green = (s: string): string => paint("32", s);
const yellow = (s: string): string => paint("33", s);
const magenta = (s: string): string => paint("35", s);
const gray = (s: string): string => paint("90", s);

function fmtMs(ms: number): string {
  return ms < 1000 ? `${String(Math.round(ms))}ms` : `${(ms / 1000).toFixed(2)}s`;
}
function fmtBytes(n: number): string {
  if (n < 1024) return `${String(n)} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

/** The payload-blind aggregate `/stats` returns, mirrored for the CLI. */
export interface CliStats {
  ops: number;
  frames: number;
  bytes: number;
  subscriptions: number;
  liveSubscriptions: number;
  malformed: number;
  orphaned: number;
  retryStorms: number;
  truncated: number;
  evictedTraces: number;
  droppedByHost: number;
  codecMismatch: boolean;
  sensitive: number;
  out: number;
  in: number;
  avgDurationMs: number;
  maxDurationMs: number;
  topMethods: { method: string; count: number }[];
}

/** One-line aggregate summary, the terminal form of the inspector's summary strip. */
export function formatStats(s: CliStats): string {
  const parts = [
    `${bold(String(s.ops))} ${dim("ops")}`,
    `${bold(String(s.frames))} ${dim(`frames (${String(s.out)}▶ ${String(s.in)}◀)`)}`,
    `${bold(fmtBytes(s.bytes))} ${dim("data")}`,
    `${bold(String(s.subscriptions))} ${dim("subs")}${s.liveSubscriptions ? ` ${green(`(${String(s.liveSubscriptions)} live)`)}` : ""}`,
    // "observed": these times are the debugger's own WS-arrival clock, so they
    // include transport + queueing delay and are not the true host call latency.
    `${bold(fmtMs(s.avgDurationMs))} ${dim(`avg (max ${fmtMs(s.maxDurationMs)}, observed)`)}`,
    s.sensitive
      ? red(`\u{1f512} ${String(s.sensitive)} sensitive`)
      : dim("\u{1f512} 0 sensitive"),
  ];
  if (s.malformed) parts.push(red(`${String(s.malformed)} malformed`));
  if (s.orphaned) parts.push(yellow(`${String(s.orphaned)} orphaned`));
  if (s.retryStorms) parts.push(yellow(`${String(s.retryStorms)} retry-storms`));
  // Loss the op list can't show: frames dropped within a kept op, whole ops
  // evicted, and frames the host dropped before delivery. A codec mismatch means
  // a connected host's wire contract differs, so its method names may be wrong.
  if (s.truncated) parts.push(yellow(`${String(s.truncated)} truncated`));
  if (s.evictedTraces) parts.push(yellow(`${String(s.evictedTraces)} evicted`));
  if (s.droppedByHost) parts.push(yellow(`${String(s.droppedByHost)} dropped`));
  if (s.codecMismatch) parts.push(red("⚠ codec mismatch"));
  return parts.join(dim(" · "));
}

const SUBSCRIPTION_ROLES = new Set(["start", "receive", "stop", "interrupt"]);

/**
 * One op as a single row: the terminal form of an op-list row. When
 * `showChannel` is set (an unscoped, multi-host view), the channel is shown so
 * two hosts minting the same `requestId` are distinguishable.
 */
export function formatOpRow(view: TraceView, showChannel = false): string {
  const sub = view.frames.some((f) => SUBSCRIPTION_ROLES.has(f.role));
  const live = sub && !view.frames.some((f) => f.role === "stop");
  const kind = sub ? magenta("⟳") : yellow("▶");
  const method = bold(viewMethod(view).padEnd(38).slice(0, 38));
  const lock = view.sensitive ? red(" \u{1f512}") : "  ";
  const badges = view.badges
    .map((b) => (b === "malformed" ? red(`[${b}]`) : yellow(`[${b}]`)))
    .join(" ");
  const meta = dim(
    `${String(view.frames.length)}f · ${live ? green("live ") : ""}${fmtMs(view.durationMs)}`,
  );
  const chan =
    showChannel && view.channelId !== undefined
      ? gray(`[${view.channelId}] `)
      : "";
  return `${kind} ${method}${lock} ${meta}${badges ? ` ${badges}` : ""}  ${chan}${gray(view.requestId)}`;
}

/** One op's full frame sequence + any resolved decode values (drill-down). */
export function formatOpDetail(
  view: TraceView,
  decoded: ReadonlyMap<number, FrameValueDetail>,
): string {
  const lines: string[] = [];
  // Durations here (and the per-frame ⟳/+ below) are the debugger's own
  // WS-arrival clock — transport + queueing included — so label them "observed"
  // rather than let a Network-tab-shaped readout imply true host call latency.
  lines.push(
    `${bold(viewMethod(view))}  ${dim(`${view.requestId} · ${String(view.frames.length)} frames · ${fmtMs(view.durationMs)} observed`)}${view.sensitive ? red("  \u{1f512} sensitive") : ""}`,
  );
  for (const f of view.frames) {
    lines.push(formatFrameRow(f));
    const detail = decoded.get(f.seq);
    if (detail) lines.push(indent(formatFrameValue(detail)));
  }
  return lines.join("\n");
}

function formatFrameRow(f: TraceFrameView): string {
  const glyph = f.direction === "out" ? yellow("▶") : green("◀");
  const role = dim(f.role.padEnd(8).slice(0, 8));
  const method = f.method ?? `id ${String(f.frameId ?? "?")}`;
  const size = f.byteLength === undefined ? "" : dim(`${String(f.byteLength)}B`);
  const lat =
    f.roundTripMs !== undefined
      ? dim(`⟳${fmtMs(f.roundTripMs)}`)
      : dim(`+${fmtMs(f.latencyFromStartMs)}`);
  return `  ${glyph} ${role} ${method}  ${size}  ${lat}`;
}

/** Render one {@link FrameValueDetail}; a revealed sensitive value is flagged. */
export function formatFrameValue(detail: FrameValueDetail): string {
  switch (detail.kind) {
    case "redacted":
      return red(
        `redacted · ${detail.reason} · ${String(detail.byteLength)}B withheld`,
      );
    case "bytes":
      return dim(`${String(detail.byteLength)}B · payload not shown`);
    case "decoded": {
      const body = JSON.stringify(
        detail.value,
        (_key, v: unknown) => (typeof v === "bigint" ? `${v.toString()}n` : v),
        2,
      );
      return detail.sensitive === true
        ? `${red("⚠ revealed sensitive material — dev only")}\n${body}`
        : body;
    }
  }
}

function indent(s: string): string {
  return s
    .split("\n")
    .map((l) => `      ${l}`)
    .join("\n");
}
