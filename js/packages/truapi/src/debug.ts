// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Host-agnostic wire-debug relay (sdk-team #26 concept).
 *
 * The transport surfaces every dispatched frame through the optional
 * {@link TransportObserver} hook (`createTransport(provider, { observe })`),
 * keyed on the wire `requestId`. This module turns that emit-only stream into a
 * usable debugger surface:
 *
 *  - {@link createWireDebugger} accumulates frames into per-`requestId` traces
 *    so a single op can be reconstructed across product → wire → host;
 *  - the same `requestId` is the value the product-sdk telemetry spans correlate
 *    on (`HostOpEvent.correlationId`), so a frame trace and a product span line
 *    up under one id with no extra plumbing;
 *  - it logs/relays each frame and never touches decoded payloads, so it works
 *    against any host without knowing the application protocol.
 *
 * The "observe + forward" path is the must-have here. Rewriting/mocking frames
 * (a full MITM) is a stretch goal layered on the same seam later.
 *
 * @module
 */

import type { ObservedFrame, TransportObserver } from "./client.js";

/** A single op's frames, in arrival order, grouped by their shared `requestId`. */
export interface WireTrace {
  /** Correlation id shared by every frame in this trace. */
  requestId: string;
  /** Frames observed for this id, in the order they crossed the transport. */
  frames: ObservedFrame[];
  /** Epoch ms of the first frame. */
  startedAt: number;
  /** Epoch ms of the most recent frame. */
  lastAt: number;
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
 * `ACCOUNT_GET_ACCOUNT = { request: 22, response: 23 }`); the service list —
 * typically `Object.keys(createClient(transport))` — disambiguates where the
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
   * Reverse map from wire `frameId` to method name (build one with
   * {@link createMethodNameMap}). When set, formatted lines carry
   * `account.getAccount` instead of a bare `id=22`.
   */
  methodNames?: ReadonlyMap<number, WireMethodInfo>;
}

/** A live wire debugger: an `observe` hook plus per-`requestId` trace lookup. */
export interface WireDebugger {
  /**
   * The `observe` callback to hand to `createTransport(provider, { observe })`.
   */
  readonly observe: TransportObserver;
  /** All retained traces, most-recently-active last. */
  traces(): WireTrace[];
  /** The trace for a specific `requestId` (e.g. a product span's correlationId). */
  trace(requestId: string): WireTrace | undefined;
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
 * Build a {@link WireDebugger}. Hand its {@link WireDebugger.observe} to
 * `createTransport`'s `observe` option to start recording. Frames are logged
 * through `sink`, forwarded through `forward` (if set), and grouped into
 * per-`requestId` {@link WireTrace}s for correlation with product-sdk spans.
 */
export function createWireDebugger(options: WireDebuggerOptions = {}): WireDebugger {
  const sink: WireDebugSink =
    options.sink ?? ((line) => console.debug(line));
  const forward = options.forward;
  const maxTraces = options.maxTraces ?? 256;
  const methodNames = options.methodNames;
  // Insertion-ordered; re-inserting on activity keeps the map LRU-ordered.
  const traces = new Map<string, WireTrace>();

  const observe: TransportObserver = (frame) => {
    let trace = traces.get(frame.requestId);
    if (trace) {
      traces.delete(frame.requestId);
    } else {
      trace = {
        requestId: frame.requestId,
        frames: [],
        startedAt: frame.timestamp,
        lastAt: frame.timestamp,
      };
    }
    trace.frames.push(frame);
    trace.lastAt = frame.timestamp;
    traces.set(frame.requestId, trace);

    while (traces.size > maxTraces) {
      const oldest = traces.keys().next().value;
      if (oldest === undefined) break;
      traces.delete(oldest);
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
    trace: (requestId) => traces.get(requestId),
    clear: () => traces.clear(),
  };
}
