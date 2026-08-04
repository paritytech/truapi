// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Trace engine: group a stream of observed frames into per-op traces.
 *
 * The host tap streams every product↔host frame to the debugger, where
 * {@link createDebugIngest} decodes each into an {@link ObservedFrame} keyed on
 * the wire `requestId`. This module turns that stream into a usable surface:
 *
 *  - {@link createWireDebugger} accumulates frames into per-`requestId` traces
 *    so a single op can be reconstructed across product → wire → host;
 *  - the same `requestId` is the value product-sdk telemetry spans correlate on
 *    (`HostOpEvent.correlationId`), so a frame trace and a product span line up
 *    under one id with no extra plumbing;
 *  - it logs/relays each frame and never touches decoded payloads, so it works
 *    against any host without knowing the application protocol.
 *
 * @module
 */

import type { ObservedFrame, TransportObserver } from "./observed-frame.js";

/**
 * A single op's frames, in arrival order, grouped by their shared
 * `(channelId, requestId)`. `requestId` alone is not unique across channels -
 * each host mints its own `p:1`, `p:2`, … - so the channel is part of a trace's
 * identity.
 */
export interface WireTrace {
  /** Product channel this op belongs to, e.g. `"myapp.dot"`. */
  channelId: string;
  /**
   * Correlation id shared by every frame in this trace. A product may recycle it
   * for a later, unrelated call; {@link WireTrace.generation} disambiguates the
   * successive ops that then share it.
   */
  requestId: string;
  /** Frames observed for this id, in the order they crossed the transport. */
  frames: ObservedFrame[];
  /** Epoch ms of the first frame. */
  startedAt: number;
  /** Epoch ms of the most recent frame. */
  lastAt: number;
  /**
   * Which reuse of `(channelId, requestId)` this op is, from `0`. A fresh opener
   * (`request`/`start`) arriving after the id's current op already opened starts
   * the next generation, so a recycled id never merges two unrelated calls.
   */
  generation: number;
  /**
   * Whether older frames were dropped from this trace to stay under the frame or
   * byte cap. Surfaced as a `truncated` op badge so the operator can tell "older
   * frames dropped" from a genuinely short op.
   */
  truncated: boolean;
}

/** Sink for fully-formatted debug lines (defaults to `console.debug`). */
export type WireDebugSink = (line: string, frame: ObservedFrame) => void;

/** Which of a method's wire ids a given `frameId` is. */
export type WireFrameKind =
  | "request"
  | "response"
  | "start"
  | "stop"
  | "receive"
  | "interrupt";

/** Resolution of a bare wire `frameId` to its human-readable method. */
export interface WireMethodInfo {
  /** Dotted method path as it appears on the client, e.g. `"account.getAccount"`. */
  method: string;
  /** Which of the method's wire ids this `frameId` is. */
  kind: WireFrameKind;
}

/** `camelCase` → `CONST_CASE`, matching the wire-table's constant naming. */
function constCase(name: string): string {
  return name.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toUpperCase();
}

/** `GET_ACCOUNT` → `getAccount`. */
function camelCase(constName: string): string {
  const [head, ...rest] = constName.toLowerCase().split("_");
  return (
    (head ?? "") +
    rest.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join("")
  );
}

/**
 * Build a reverse map from wire `frameId` to `"service.method"` name out of the
 * generated wire-table module and the client's service names.
 *
 * The wire-table exports one `CONST_CASE` group per method (e.g.
 * `ACCOUNT_GET_ACCOUNT = { request: 22, response: 23 }`); the service list -
 * typically `Object.keys(createClient(transport))` - disambiguates where the
 * service prefix ends (`LOCAL_STORAGE_READ` → `localStorage.read`, not
 * `local.storageRead`). Non-group exports in `table` are ignored, so the whole
 * `import * as W from "./generated/wire-table.js"` namespace can be passed
 * directly.
 */
export function createMethodNameMap(
  table: Record<string, unknown>,
  services: readonly string[],
): ReadonlyMap<number, WireMethodInfo> {
  // Longest prefix first, so RESOURCE_ALLOCATION_ wins over a hypothetical RESOURCE_.
  const prefixes = services
    .map((service) => ({ service, prefix: `${constCase(service)}_` }))
    .sort((a, b) => b.prefix.length - a.prefix.length);

  const map = new Map<number, WireMethodInfo>();
  for (const [constName, group] of Object.entries(table)) {
    if (group === null || typeof group !== "object") continue;
    const match = prefixes.find(({ prefix }) => constName.startsWith(prefix));
    const method = match
      ? `${match.service}.${camelCase(constName.slice(match.prefix.length))}`
      : camelCase(constName);
    for (const [kind, id] of Object.entries(group)) {
      if (typeof id !== "number") continue;
      map.set(id, { method, kind: kind as WireFrameKind });
    }
  }
  return map;
}

/** Options for {@link createWireDebugger}. */
export interface WireDebuggerOptions {
  /**
   * Where formatted frame lines go. Defaults to `console.debug`. A host-side
   * panel (e.g. dotli's wire-debug view) passes its own sink here to render the
   * stream live.
   */
  sink?: WireDebugSink;
  /**
   * Optional forward target: a second observer to receive every frame after it
   * is recorded. Lets a host relay frames onward (to a panel, a socket, an OTel
   * exporter) while the debugger keeps its own per-id traces.
   */
  forward?: TransportObserver;
  /** Cap on retained traces (LRU-evicted). Default 256. */
  maxTraces?: number;
  /**
   * Cap on retained frames within a single trace (oldest ring-buffered out).
   * Default 1024. Without this a long-lived subscription - e.g.
   * `account.connectionStatus`, which shares one `requestId` for the whole
   * session - accumulates a frame per `receive` forever, since all its frames
   * share a `requestId` that never LRU-evicts from {@link maxTraces}. A panel
   * showing the last N frames of a subscription is no worse than one showing
   * all of them.
   */
  maxFramesPerTrace?: number;
  /**
   * Cap on total retained payload bytes within a single trace. Only bites when
   * the ingest retains bytes (level-2 decode); with decode off, frames carry no
   * bytes and this never triggers. Without it, a burst of large payloads sharing
   * one long-lived `requestId` grows memory unbounded even under
   * {@link maxFramesPerTrace} (count-capped, not byte-capped). Oldest non-opener
   * frames are evicted until the trace is under budget. Default 1 MiB.
   */
  maxBytesPerTrace?: number;
  /**
   * Reverse map from wire `frameId` to method name (build one with
   * {@link createMethodNameMap}). When set, formatted lines carry
   * `account.getAccount` instead of a bare `id=22`.
   */
  methodNames?: ReadonlyMap<number, WireMethodInfo>;
}

/** A live wire debugger: an `observe` hook plus per-`(channelId, requestId)` trace lookup. */
export interface WireDebugger {
  /** The callback that records a frame; drive it from {@link createDebugIngest}. */
  readonly observe: TransportObserver;
  /** All retained traces across all channels, most-recently-active last. */
  traces(): WireTrace[];
  /**
   * The current (latest-generation) trace for a `requestId`. Pass `channelId` to
   * disambiguate when more than one host is connected (each mints the same `p:N`
   * ids); without it, the most-recently-active op matching `requestId` is returned
   * - fine for a single-host session or product-span (`correlationId`) correlation.
   */
  trace(
    requestId: string,
    channelId?: string,
    generation?: number,
  ): WireTrace | undefined;
  /** All retained traces for one channel, most-recently-active last. */
  tracesForChannel(channelId: string): WireTrace[];
  /**
   * Count of whole operations LRU-evicted since the last {@link clear}. Distinct
   * from per-op frame truncation ({@link WireTrace.truncated}): whole-op eviction
   * is otherwise invisible because {@link traces} shows only survivors, so this
   * is how a consumer tells "kept 256 of 10k" from "only 256 ever happened".
   */
  evictedTraces(): number;
  /** Drop all retained traces. */
  clear(): void;
}

function formatFrame(
  frame: ObservedFrame,
  methodNames?: ReadonlyMap<number, WireMethodInfo>,
): string {
  const arrow = frame.direction === "out" ? "→" : "←";
  const method = methodNames?.get(frame.frameId)?.method;
  const label = method ? `${frame.role} ${method}` : frame.role;
  return `[wire ${frame.requestId}] ${arrow} ${label} (id=${frame.frameId}, ${frame.byteLength}B)`;
}

/**
 * Build a {@link WireDebugger}. Feed its {@link WireDebugger.observe} from
 * {@link createDebugIngest} to start recording. Frames are logged through
 * `sink`, forwarded through `forward` (if set), and grouped into
 * per-`requestId` {@link WireTrace}s for correlation with product-sdk spans.
 */
export function createWireDebugger(options: WireDebuggerOptions = {}): WireDebugger {
  const sink: WireDebugSink =
    options.sink ?? ((line) => console.debug(line));
  const forward = options.forward;
  const maxTraces = options.maxTraces ?? 256;
  const maxFramesPerTrace = options.maxFramesPerTrace ?? 1024;
  const maxBytesPerTrace = options.maxBytesPerTrace ?? 1024 * 1024;
  const methodNames = options.methodNames;
  // Insertion-ordered; re-inserting on activity keeps the map LRU-ordered.
  // Keyed by `(channelId, requestId)` since requestId is per-channel only.
  const traces = new Map<string, WireTrace>();
  const keyOf = (channelId: string, requestId: string): string =>
    `${channelId}\u0000${requestId}`;

  // `(channelId, requestId)` -> the gen-key of that id's current (latest) op.
  const current = new Map<string, string>();
  // Whole operations LRU-evicted since the last clear(). Surfaced so a session
  // that overflowed maxTraces doesn't silently under-report its op count.
  let evictedCount = 0;
  // A frame's lifecycle role. The ingest leaves it "unknown" (lifecycle isn't on
  // the wire), so fall back to the frameId's wire-table kind — the same resolution
  // wireTraceToView uses — otherwise no real frame ever reads as an opener.
  const roleOf = (f: ObservedFrame): string | undefined =>
    f.role !== "unknown" ? f.role : methodNames?.get(f.frameId)?.kind;
  // A frame that begins an operation: a unary request or a subscription start.
  const isOpener = (f: ObservedFrame): boolean => {
    const r = roleOf(f);
    return r === "request" || r === "start";
  };

  const observe: TransportObserver = (frame) => {
    const baseKey = keyOf(frame.channelId, frame.requestId);
    const curKey = current.get(baseKey);
    const cur = curKey !== undefined ? traces.get(curKey) : undefined;

    // A fresh opener for an id whose current op already opened means the product
    // recycled the requestId: rotate to a new generation so the two never merge.
    const rotate =
      cur !== undefined &&
      isOpener(frame) &&
      cur.frames.some((f) => isOpener(f));

    let trace: WireTrace;
    let key: string;
    if (curKey !== undefined && cur !== undefined && !rotate) {
      traces.delete(curKey); // re-insert below to keep the map LRU-ordered
      trace = cur;
      key = curKey;
    } else {
      const generation = cur === undefined ? 0 : cur.generation + 1;
      key = `${baseKey} ${String(generation)}`;
      trace = {
        channelId: frame.channelId,
        requestId: frame.requestId,
        generation,
        frames: [],
        startedAt: frame.timestamp,
        lastAt: frame.timestamp,
        truncated: false,
      };
    }
    trace.frames.push(frame);
    if (trace.frames.length > maxFramesPerTrace) {
      // Evict oldest to keep an exact hard cap, but NEVER the opener (frames[0]):
      // it is the request/start the pairing (`orphaned`) and retry-storm signals
      // key on, so dropping it would falsely orphan a long-lived subscription
      // (e.g. account.connectionStatus). Ring-buffer from index 1 instead.
      trace.frames.splice(1, trace.frames.length - maxFramesPerTrace);
      trace.truncated = true;
    }
    // Byte cap: only bites when bytes are retained (level-2 decode). Evict oldest
    // non-opener frames until the retained payload is under budget, so one id's
    // large payloads can't grow memory without bound even under the count cap.
    if (frame.bytes !== undefined && maxBytesPerTrace !== Infinity) {
      let retained = 0;
      for (const f of trace.frames) retained += f.bytes?.length ?? 0;
      while (retained > maxBytesPerTrace && trace.frames.length > 1) {
        const [removed] = trace.frames.splice(1, 1);
        retained -= removed?.bytes?.length ?? 0;
        trace.truncated = true;
      }
    }
    trace.lastAt = frame.timestamp;
    traces.set(key, trace);
    current.set(baseKey, key);

    while (traces.size > maxTraces) {
      const oldest = traces.keys().next().value;
      if (oldest === undefined) break;
      const evicted = traces.get(oldest);
      traces.delete(oldest);
      evictedCount += 1;
      // If the evicted op was an id's current, forget it so reuse starts clean.
      if (evicted !== undefined) {
        const bk = keyOf(evicted.channelId, evicted.requestId);
        if (current.get(bk) === oldest) current.delete(bk);
      }
    }

    try {
      sink(formatFrame(frame, methodNames), frame);
    } catch {
      // A debug sink must never break the observed transport.
    }
    if (forward) {
      try {
        forward(frame);
      } catch {
        // A forward target must never break the observed transport.
      }
    }
  };

  return {
    observe,
    traces: () => [...traces.values()],
    trace: (requestId, channelId, generation) => {
      // A specific generation (drill-down into one op of a recycled id).
      if (generation !== undefined) {
        for (const t of traces.values()) {
          if (
            t.requestId === requestId &&
            t.generation === generation &&
            (channelId === undefined || t.channelId === channelId)
          ) {
            return t;
          }
        }
        return undefined;
      }
      // The current (latest) generation for this id.
      if (channelId !== undefined) {
        const key = current.get(keyOf(channelId, requestId));
        return key !== undefined ? traces.get(key) : undefined;
      }
      // No channel given: the most recent op matching this requestId. Iterate in
      // LRU order and keep the last match, so a reused id resolves to its latest op.
      let match: WireTrace | undefined;
      for (const t of traces.values()) {
        if (t.requestId === requestId) match = t;
      }
      return match;
    },
    tracesForChannel: (channelId) =>
      [...traces.values()].filter((t) => t.channelId === channelId),
    evictedTraces: () => evictedCount,
    clear: () => {
      traces.clear();
      current.clear();
      evictedCount = 0;
    },
  };
}
