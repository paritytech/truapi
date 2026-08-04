// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Shared client for the terminal frontends (the one-shot {@link module:cli}
 * commands and the interactive {@link module:repl}). Reads a running debugger's
 * HTTP endpoints and rebuilds the shared {@link TraceView} model, so both
 * frontends agree with the web inspector on ops, badges, sensitivity, and what
 * may be decoded - one engine, one denylist, no forks.
 *
 * @module
 */

import { SENSITIVE_FRAME_IDS, type FrameValueDetail } from "./decode.js";
import type { FrameRole } from "./observed-frame.js";
import {
  buildTraceView,
  type TraceBadge,
  type TraceView,
  type TraceViewInput,
} from "./trace-view.js";
import type { CliStats } from "./trace-text.js";

/** The sensitive denylist, resolved once from the generated wire-table. */
export const sensitiveIds = SENSITIVE_FRAME_IDS;

/** One frame as `/traces` serializes it (payload-blind: no bytes, no values). */
export interface TracesFrame {
  direction: "out" | "in";
  frameId: number;
  method?: string;
  role: string;
  byteLength?: number;
  timestamp: number;
}
/** One op as `/traces` serializes it. */
export interface TracesEntry {
  channelId: string;
  requestId: string;
  /** Which reuse of `(channelId, requestId)` this op is; see {@link TraceView.generation}. */
  generation?: number;
  startedAt: number;
  lastAt: number;
  /** Op-level badges the server computed (incl. the cross-op retry-storm). */
  badges?: TraceBadge[];
  frames: TracesFrame[];
}
/** One host as `/channels` reports it. */
export interface ChannelInfo {
  channelId: string;
  connected: boolean;
  frameCount: number;
}

export type { CliStats, FrameValueDetail };

/** Rebuild the shared view model from a payload-blind `/traces` entry. */
export function toView(entry: TracesEntry): TraceView {
  const input: TraceViewInput = {
    requestId: entry.requestId,
    channelId: entry.channelId,
    generation: entry.generation,
    startedAt: entry.startedAt,
    lastAt: entry.lastAt,
    // Cross-op badges (retry-storm) are computed server-side and passed through,
    // so the CLI shows the same badges as the web inspector without recomputing.
    extraBadges: entry.badges,
    frames: entry.frames.map((f) => ({
      direction: f.direction,
      // `/traces` role strings come straight off the engine's FrameRole union.
      role: f.role as FrameRole,
      method: f.method,
      frameId: f.frameId,
      byteLength: f.byteLength,
      timestamp: f.timestamp,
      decodable: false,
      sensitive: sensitiveIds.has(f.frameId),
    })),
  };
  return buildTraceView(input);
}

export { viewMethod } from "./trace-view.js";

/** A thin HTTP client over a running debugger server. */
export interface DebuggerClient {
  readonly host: string;
  traces(): Promise<TracesEntry[]>;
  stats(channel: string | null): Promise<CliStats>;
  channels(): Promise<ChannelInfo[]>;
  /**
   * The gated per-frame drill-down. `reveal` is honored only when the server
   * armed `TRUAPI_DEBUGGER_REVEAL_SENSITIVE`; otherwise a sensitive frame still
   * comes back redacted - the guarantee lives server-side, not here.
   */
  frame(
    requestId: string,
    seq: number,
    channel: string | null,
    reveal: boolean,
  ): Promise<FrameValueDetail>;
}

/** Build a {@link DebuggerClient} for `host` (e.g. `http://localhost:9231`). */
export function createDebuggerClient(host: string): DebuggerClient {
  const getJson = async <T>(path: string): Promise<T> => {
    const res = await fetch(host + path);
    if (!res.ok) throw new Error(`${host}${path} → HTTP ${String(res.status)}`);
    return res.json() as Promise<T>;
  };
  const channelQuery = (channel: string | null): string =>
    channel ? `?channel=${encodeURIComponent(channel)}` : "";
  return {
    host,
    traces: () => getJson<TracesEntry[]>("/traces"),
    stats: (channel) => getJson<CliStats>(`/stats${channelQuery(channel)}`),
    channels: async () =>
      (await getJson<{ channels: ChannelInfo[] }>("/channels")).channels,
    frame: (requestId, seq, channel, reveal) => {
      const p = new URLSearchParams({ id: requestId, i: String(seq) });
      if (channel) p.set("channel", channel);
      if (reveal) p.set("reveal", "1");
      return getJson<FrameValueDetail>(`/frame?${p.toString()}`);
    },
  };
}
