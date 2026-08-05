// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Retry-storm detection: a *cross-op* signal the single-trace renderer cannot
 * see on its own.
 *
 * A retry storm is a burst of like ops in a short window — a product hammering
 * `signing.createTransaction` five times in 400ms because each attempt failed,
 * say. Whether any one op is part of a storm depends on the *other* traces, so
 * it belongs in the engine/list layer, not the per-trace renderer. This module
 * computes it over the whole trace set and hands each stormed trace a
 * `retry-storm` {@link TraceBadge}, which the mount feeds to `wireTraceToView`'s
 * `extraBadges`. The renderer stays display-only.
 *
 * @module
 */

import type { TraceBadge } from "./trace-view.js";
import type { WireTrace } from "./wire-debugger.js";

/** Tuning for {@link detectRetryStorms}. */
export interface RetryStormOptions {
  /**
   * The window, in ms, within which like ops count as one burst. Default 1000.
   */
  windowMs?: number;
  /**
   * How many like ops within `windowMs` make a storm. Default 3. Values below 2
   * are meaningless (a single op is never a storm) and detect nothing.
   */
  minBurst?: number;
}

/**
 * The op signature two traces must share to count as "like". A storm is one host
 * hammering one method, so the signature is scoped to the channel: `channelId`
 * plus the opener frame's wire `frameId` (the first frame is the `request`/`start`,
 * so its id identifies the method). Same channel + same op id = the same op being
 * repeated; two different hosts each firing the op once is not a storm. A trace
 * with no frames has no signature and never storms.
 */
function signature(trace: WireTrace): string | undefined {
  const frameId = trace.frames[0]?.frameId;
  return frameId === undefined ? undefined : `${trace.channelId}\u0000${frameId}`;
}

/**
 * Find every trace that is part of a retry storm and map it to its badge.
 *
 * Traces are grouped by op {@link signature}; within each group, a sliding
 * window over `startedAt` flags any trace that sits in a span of `minBurst` or
 * more ops no wider than `windowMs`. The result is keyed by the {@link WireTrace}
 * object itself (not `requestId`, which is not unique across channels): only
 * stormed traces appear, each mapped to `["retry-storm"]`. Feed
 * `result.get(trace) ?? []` into `wireTraceToView`'s `extraBadges`.
 */
export function detectRetryStorms(
  traces: readonly WireTrace[],
  options: RetryStormOptions = {},
): ReadonlyMap<WireTrace, readonly TraceBadge[]> {
  const windowMs = options.windowMs ?? 1000;
  const minBurst = options.minBurst ?? 3;
  const result = new Map<WireTrace, readonly TraceBadge[]>();
  if (minBurst < 2) return result;

  const groups = new Map<string, WireTrace[]>();
  for (const trace of traces) {
    const sig = signature(trace);
    if (sig === undefined) continue;
    const group = groups.get(sig);
    if (group) group.push(trace);
    else groups.set(sig, [trace]);
  }

  for (const group of groups.values()) {
    if (group.length < minBurst) continue;
    const sorted = [...group].sort((a, b) => a.startedAt - b.startedAt);
    let left = 0;
    for (let right = 0; right < sorted.length; right++) {
      while (sorted[right].startedAt - sorted[left].startedAt > windowMs) {
        left++;
      }
      // [left, right] now spans <= windowMs, so every trace in it is within
      // windowMs of every other. If that's a full burst, they all storm.
      if (right - left + 1 >= minBurst) {
        for (let k = left; k <= right; k++) {
          result.set(sorted[k], ["retry-storm"]);
        }
      }
    }
  }

  return result;
}
