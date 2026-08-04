/**
 * The runnable debugger app: the WS server a host dials into, plus a minimal
 * trace view.
 *
 * A host's outward WS dial sends one text message per frame -
 * `{ channelId, dir, frame }`, where `frame` is the base64 of the raw SCALE
 * `ProtocolMessage` bytes (JSON can't carry binary; base64 keeps the envelope on
 * one line). Each message is decoded and grouped by {@link createDebugSession}.
 * `GET /traces` returns the grouped traces (payload-blind - raw bytes and
 * decoded values are never serialized); `GET /frame?id=&i=` is the drill-down
 * detail path, the only place a decoded value can surface, and only when level-2
 * decode is opted in (`TRUAPI_DEBUGGER_DECODE_VALUES`, off by default) and the
 * frame is not sensitive; `GET /` serves a page that polls `/traces`.
 *
 * The exact host↔debugger framing is not yet standardized (envelope spec, track
 * T3); base64-in-JSON is what this server accepts today. Runs under Bun
 * (`bun run src/server.ts`).
 *
 * @module
 */

import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import { createDebugSession } from "./session.js";
import {
  DEFAULT_MAX_ID_CHARS,
  WIRE_ENVELOPE_VERSION,
  type DebugFrameEnvelope,
} from "./ingest.js";
import { wireTraceToView, type TraceView } from "./trace-view.js";
import type { CliStats } from "./trace-text.js";
import {
  renderFrameValueDetail,
  renderOperationRow,
  renderTraceDetail,
} from "./trace-render.js";
import { detectRetryStorms } from "./retry-storm.js";
import { TRACE_DETAIL_CSS } from "./trace-styles.js";

/** Default port the debugger listens on; a host points its debug URL here. */
const DEFAULT_PORT = 9231;

/** Frame roles that make an op a subscription rather than a request/response. */
const SUBSCRIPTION_ROLES = new Set<string>([
  "start",
  "receive",
  "stop",
  "interrupt",
]);

/**
 * The text message a host sends per frame: the envelope with a base64 frame,
 * plus the optional identity fields (`v`, `codec`) a versioned host stamps.
 */
interface WireMessage {
  channelId: string;
  dir: "in" | "out";
  frame: string;
  /** Envelope version; see {@link WIRE_ENVELOPE_VERSION}. */
  v?: number;
  /** The host's wire codec version (`TRUAPI_CODEC_VERSION`). */
  codec?: number;
  /**
   * The host's wire-contract fingerprint (`TRUAPI_WIRE_SCHEMA_HASH`): a hash of
   * every frame id, its method leg, and its sensitivity. Unlike `codec` (the
   * coarse handshake number, bumped ~never), this changes whenever a frame id is
   * reassigned or a `#[wire(sensitive)]` flag flips - the case where a
   * host-sensitive frame could otherwise decode off this debugger's denylist.
   */
  schema?: string;
  /** Frames this host dropped (link backlog full) before this one; surfaced in stats. */
  dropped?: number;
}

/** A parsed inbound message: the envelope plus its wire-identity verdict. */
interface ParsedWireMessage {
  envelope: DebugFrameEnvelope;
  /**
   * `true` when the host stamped a `v`/`codec`/`schema` that does not match this
   * debugger's - the API-evolved-underneath case. Blocks the value-decode path.
   */
  identityMismatch: boolean;
  /**
   * `true` only when the host affirmatively stamped a `schema` equal to this
   * debugger's. Decode is allowed only for confirmed channels: an absent schema
   * (a foreign or pre-identity host) is NOT trusted to decode, closing the
   * omit-identity-to-bypass hole. Payload-blind grouping is unaffected.
   */
  identityConfirmed: boolean;
  /** Frames the host reported dropping before this one. */
  dropped: number;
}

/**
 * Whether a WebSocket upgrade may proceed. Non-browser clients (the CLI, curl)
 * send no Origin and are allowed; a browser sends its page Origin, which must be
 * a loopback host - a cross-origin page dialing the debugger to inject frames is
 * refused (CSWSH), which binding to loopback alone does not prevent.
 */
function originAllowed(origin: string | null): boolean {
  if (origin === null) return true;
  try {
    const host = new URL(origin).hostname;
    // `new URL("http://[::1]").hostname` keeps the brackets ("[::1]"), so match
    // that form (a bare "::1" never occurs, but accept it defensively).
    return (
      host === "127.0.0.1" ||
      host === "localhost" ||
      host === "[::1]" ||
      host === "::1"
    );
  } catch {
    return false;
  }
}

/**
 * Parse an optional integer query param: `undefined` if absent, `null` if
 * malformed. Requires a canonical integer so `""`, `" "`, `"1e3"`, `"0x10"`,
 * `"1.5"`, and `"+1"` all reject rather than silently coercing (`Number("")===0`).
 */
function optionalInt(raw: string | null): number | null | undefined {
  if (raw === null) return undefined;
  const t = raw.trim();
  if (!/^-?\d+$/.test(t)) return null;
  const n = Number(t);
  return Number.isInteger(n) ? n : null;
}

/** Parse and validate one inbound WS text message, or `null`. */
function parseWireMessage(raw: string): ParsedWireMessage | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const m = parsed as Partial<WireMessage>;
  if (typeof m.channelId !== "string") return null;
  if (m.dir !== "in" && m.dir !== "out") return null;
  if (typeof m.frame !== "string") return null;
  const schema = typeof m.schema === "string" ? m.schema : undefined;
  const identityMismatch =
    (typeof m.v === "number" && m.v !== WIRE_ENVELOPE_VERSION) ||
    (typeof m.codec === "number" && m.codec !== TRUAPI_CODEC_VERSION) ||
    (schema !== undefined && schema !== TRUAPI_WIRE_SCHEMA_HASH);
  return {
    envelope: {
      channelId: m.channelId,
      dir: m.dir,
      frame: new Uint8Array(Buffer.from(m.frame, "base64")),
    },
    identityMismatch,
    identityConfirmed: schema === TRUAPI_WIRE_SCHEMA_HASH,
    dropped: typeof m.dropped === "number" && m.dropped > 0 ? m.dropped : 0,
  };
}

/** A running debugger server. */
export interface DebugServer {
  /** The port the WS/HTTP server is listening on. */
  readonly port: number;
  /** Whether level-2 value decode is enabled on the drill-down path. */
  readonly decodeValues: boolean;
  /** Whether the dev-only sensitive-reveal escape hatch is armed. */
  readonly revealSensitive: boolean;
  /** Stop listening and drop active connections. */
  stop(): void;
}

/**
 * `JSON.stringify` that survives decoded SCALE values: `bigint` becomes a
 * decimal string and `Uint8Array` a `0x…` hex string, both of which
 * `JSON.stringify` otherwise throws on or renders as an index map. Only the
 * drill-down detail path uses this; `/traces` never serializes decoded values.
 */
function safeStringify(value: unknown): string {
  return JSON.stringify(value, (_key, val) => {
    if (typeof val === "bigint") return val.toString();
    if (val instanceof Uint8Array) {
      return `0x${Buffer.from(val).toString("hex")}`;
    }
    return val;
  });
}

/**
 * Start the debugger app: a Bun WS+HTTP server that decodes and groups every
 * frame a host streams to it. `port: 0` binds an ephemeral port, read back from
 * {@link DebugServer.port}.
 *
 * Level-2 value decode is off unless `decodeValues` is set (the CLI entry point
 * derives it from `TRUAPI_DEBUGGER_DECODE_VALUES`). It only ever affects the
 * `/frame` drill-down; `/traces` is byte- and value-free either way.
 */
export function startDebugServer(
  options: {
    port?: number;
    decodeValues?: boolean;
    revealSensitive?: boolean;
  } = {},
): DebugServer {
  const decodeValues = options.decodeValues ?? false;
  // The reveal escape hatch is meaningless without decode on; fold the master
  // gate in here so a stray env var alone can never arm it.
  const revealSensitive = decodeValues && (options.revealSensitive ?? false);
  const session = createDebugSession({ decodeValues, revealSensitive });

  /** Adapt one trace to a view with the shared method map + denylist. */
  const toView = (
    trace: ReturnType<typeof session.traceEngine.traces>[number],
    storms: ReturnType<typeof detectRetryStorms>,
  ): TraceView =>
    wireTraceToView(
      trace,
      session.methodNames,
      storms.get(trace) ?? [],
      session.sensitiveIds,
    );

  /**
   * Compute the cross-op retry-storm signal once over a trace set, then adapt
   * every trace. The `traces() → detectRetryStorms → wireTraceToView` pipeline is
   * shared by every list-level endpoint so the same aggregation runs once, not
   * per endpoint.
   */
  const viewsFor = (
    traces: ReturnType<typeof session.traceEngine.traces>,
  ): { trace: (typeof traces)[number]; view: TraceView }[] => {
    const storms = detectRetryStorms(traces);
    return traces.map((trace) => ({ trace, view: toView(trace, storms) }));
  };

  function tracesJson(): string {
    // Payload-blind view: raw `bytes` and decoded values are deliberately never
    // serialized here - decode lives only on the `/frame` drill-down. `method`
    // and `role` are public shape metadata derived from the frame id (the same
    // id→name map the op list already exposes), not payload, so they are safe.
    // Rendering each trace through the shared `wireTraceToView` also gives
    // op-level badges (incl. the cross-op retry-storm signal), so the web and
    // terminal frontends read one computed signal rather than each recomputing
    // (or, for the CLI, silently omitting) it.
    const out = viewsFor(session.traceEngine.traces()).map(({ trace: t, view }) => {
      return {
        channelId: t.channelId,
        requestId: t.requestId,
        generation: t.generation,
        startedAt: t.startedAt,
        lastAt: t.lastAt,
        badges: view.badges,
        frames: view.frames.map((f) => ({
          direction: f.direction,
          frameId: f.frameId,
          method: f.method,
          role: f.role,
          byteLength: f.byteLength,
          timestamp: f.timestamp,
        })),
      };
    });
    return JSON.stringify(out);
  }

  /** The `/frame?id=<requestId>&i=<index>[&channel=<channelId>]` drill-down detail response. */
  function frameResponse(url: URL): Response {
    const id = url.searchParams.get("id");
    const rawIndex = url.searchParams.get("i");
    const channel = url.searchParams.get("channel") ?? undefined;
    const reveal = url.searchParams.get("reveal") === "1";
    // `Number("")`/`Number(" ")` are both 0 and pass Number.isInteger, so an
    // empty or whitespace `?i=` or `?gen=` would otherwise resolve frame 0 /
    // generation 0 (the oldest recycled op) with a 200; optionalInt rejects them.
    const generation = optionalInt(url.searchParams.get("gen"));
    const index = Number(rawIndex);
    if (
      id === null ||
      rawIndex === null ||
      rawIndex.trim() === "" ||
      !Number.isInteger(index) ||
      generation === null
    ) {
      return new Response('{"error":"id and integer i required"}', {
        status: 400,
        headers: { "content-type": "application/json" },
      });
    }
    if (!decodeTrusted(channel)) return codecRefusal("application/json");
    const detail = session.frameDetail(id, index, channel, reveal, generation);
    if (!detail) {
      return new Response('{"error":"no such frame"}', {
        status: 404,
        headers: { "content-type": "application/json" },
      });
    }
    return new Response(safeStringify(detail), {
      headers: { "content-type": "application/json" },
    });
  }

  /**
   * The `/view` payload-blind level-1 fragment: every trace rendered by the
   * shared {@link renderTraceDetail}, the same renderer dotli's panel mounts.
   * No payloads here; decode controls appear per frame only when level-2 is on.
   */
  function viewHtml(): string {
    const entries = viewsFor(session.traceEngine.traces());
    if (entries.length === 0) {
      return `<div class="td-empty">no frames yet</div>`;
    }
    // Wrap each rendered op in `.td-drilldown` - dotli's verbatim card wrapper -
    // so the standalone list gets the same per-op framing without a bespoke rule.
    return entries
      .map(
        ({ view }) =>
          `<div class="td-drilldown">` +
          renderTraceDetail(view, {
            offerDecode: session.decodeValues,
            offerReveal: session.revealSensitive,
          }) +
          `</div>`,
      )
      .join("");
  }

  /**
   * The `/frame-html?id=&i=` server-rendered level-2 fragment for one frame.
   * Reuses the denylist-gated {@link DebugSession.frameDetail} and the shared
   * value renderer, so a sensitive frame renders redacted here too.
   */
  function frameHtmlResponse(url: URL): Response {
    const htmlHeaders = { "content-type": "text/html; charset=utf-8" };
    const id = url.searchParams.get("id");
    const rawIndex = url.searchParams.get("i");
    const channel = url.searchParams.get("channel") ?? undefined;
    const reveal = url.searchParams.get("reveal") === "1";
    const generation = optionalInt(url.searchParams.get("gen"));
    const index = Number(rawIndex);
    if (
      id === null ||
      rawIndex === null ||
      rawIndex.trim() === "" ||
      !Number.isInteger(index) ||
      generation === null
    ) {
      return new Response(`<div class="td-bytes-only">bad request</div>`, {
        status: 400,
        headers: htmlHeaders,
      });
    }
    if (!decodeTrusted(channel)) {
      return new Response(
        `<div class="td-bytes-only">decode refused — host wire codec mismatch</div>`,
        { status: 409, headers: htmlHeaders },
      );
    }
    const detail = session.frameDetail(id, index, channel, reveal, generation);
    if (!detail) {
      return new Response(`<div class="td-bytes-only">no such frame</div>`, {
        status: 404,
        headers: htmlHeaders,
      });
    }
    return new Response(renderFrameValueDetail(detail), {
      headers: htmlHeaders,
    });
  }

  // Per-channel liveness for the inspector's host dimension. The envelope
  // carries channelId; recording first/last-seen + frame count lets the UI show
  // which hosts have dialed in and whether they are still active. Grouping
  // traces by channel is a separate engine concern; this is only connection
  // state.
  //
  // `connected` is RECENCY-based, not socket-based: a host counts as connected
  // if it emitted a frame within the last CONNECTED_WINDOW_MS. It is NOT "has an
  // open WS socket" - one WS can multiplex frames for several channelIds, so
  // per-host socket liveness is not a clean fact. A host that goes quiet without
  // closing its socket correctly reads as not-connected after the window.
  const CONNECTED_WINDOW_MS = 5000;
  // Cap the registry so a host (or anything able to reach the port) emitting
  // frames under many distinct channelIds can't grow it without bound; when
  // full, evict the least-recently-seen channel.
  const MAX_CHANNELS = 256;
  // Clamp channelId to the same bound ingest uses so this registry's key matches
  // the trace-engine key the UI filters by, and an over-long attacker-chosen id
  // can't bloat the map (256 entries * an unbounded key would otherwise grow it).
  const clampChannelId = (id: string): string =>
    id.length > DEFAULT_MAX_ID_CHARS ? id.slice(0, DEFAULT_MAX_ID_CHARS) : id;
  const channels = new Map<
    string,
    {
      channelId: string;
      firstSeen: number;
      lastSeen: number;
      frameCount: number;
      // `false` once this host has sent a frame whose declared wire identity
      // (`v`/`codec`/`schema`) does not match this debugger's. Sticky: a single
      // mismatch marks the host untrusted for the rest of the session.
      codecOk: boolean;
      // `true` once this host affirmatively stamped a matching `schema`. Decode
      // requires it, so a host that never declares identity is refused, not
      // trusted by omission.
      schemaOk: boolean;
      // Frames the host reported dropping before delivery (its link backlog
      // filled): a gap attributable to the link, surfaced so it is not read as
      // the host "not answering".
      dropped: number;
    }
  >();
  let openSockets = 0;
  // Sticky: any host has sent an unconfirmed (mismatched or unstamped) frame this
  // session. The no-channel decode path keys on this rather than scanning the live
  // registry, because an untrusted host's channel record can be LRU-evicted (see
  // MAX_CHANNELS) while its frames survive in the trace engine.
  let sawUntrusted = false;

  function recordChannel(channelId: string, parsed: ParsedWireMessage): void {
    if (!parsed.identityConfirmed) sawUntrusted = true;
    const now = Date.now();
    const key = clampChannelId(channelId);
    const existing = channels.get(key);
    if (existing) {
      existing.lastSeen = now;
      existing.frameCount += 1;
      existing.dropped += parsed.dropped;
      if (parsed.identityMismatch) existing.codecOk = false;
      if (parsed.identityConfirmed) existing.schemaOk = true;
      return;
    }
    if (channels.size >= MAX_CHANNELS) {
      let oldestKey: string | undefined;
      let oldestSeen = Infinity;
      for (const [k, c] of channels) {
        if (c.lastSeen < oldestSeen) {
          oldestSeen = c.lastSeen;
          oldestKey = k;
        }
      }
      if (oldestKey !== undefined) channels.delete(oldestKey);
    }
    channels.set(key, {
      channelId: key,
      firstSeen: now,
      lastSeen: now,
      frameCount: 1,
      codecOk: !parsed.identityMismatch,
      schemaOk: parsed.identityConfirmed,
      dropped: parsed.dropped,
    });
  }

  /**
   * Whether a decoded value may be surfaced for a channel's frames. Only bites
   * when decode is on (payload-blind mode never decodes anyway). Decode is
   * allowed only for a channel that affirmatively stamped a matching wire
   * `schema` and never mismatched.
   *
   * This is a COMPATIBILITY guard against honest version drift - a host built
   * against a different frame table, where a host-sensitive id could resolve off
   * this debugger's `SENSITIVE_FRAME_IDS` - not authentication:
   * `TRUAPI_WIRE_SCHEMA_HASH` is a public build constant, so a deliberate local
   * injector could stamp it. The WS Origin gate ({@link originAllowed}) is the
   * boundary against injection; this is defence in depth on top of it.
   */
  function decodeTrusted(channel: string | undefined): boolean {
    if (!decodeValues) return true;
    if (channel !== undefined) {
      const c = channels.get(clampChannelId(channel));
      return c !== undefined && c.codecOk && c.schemaOk;
    }
    // No channel disambiguator: refuse once any host has been untrusted this
    // session (sticky, so an evicted untrusted record can't launder its surviving
    // frames). An all-trusted or empty session stays true, so a missing frame
    // 404s rather than being masked by a refusal.
    return !sawUntrusted;
  }

  /** The 409 a decode path returns when the source host's wire codec mismatches. */
  function codecRefusal(contentType: string): Response {
    return new Response('{"error":"decode refused: host wire codec mismatch"}', {
      status: 409,
      headers: { "content-type": contentType },
    });
  }

  function channelsJson(): string {
    const now = Date.now();
    const list = [...channels.values()].sort((a, b) => b.lastSeen - a.lastSeen);
    return JSON.stringify({
      sockets: openSockets,
      // A banner signal: at least one connected host is streaming a wire codec
      // this debugger can't decode against.
      codecMismatch: list.some((c) => !c.codecOk),
      channels: list.map((c) => ({
        ...c,
        connected: now - c.lastSeen < CONNECTED_WINDOW_MS,
      })),
    });
  }

  /**
   * The `/stats?channel=` aggregate roll-up over the ops being listed: counts,
   * byte totals, durations, health-badge tallies, the request/response split,
   * and the busiest methods. Payload-blind - it sums shape and timing only and
   * never serializes a byte or a decoded value. Feeds the inspector's summary
   * strip (the "aggregate-level value").
   */
  function statsJson(channel: string | null): string {
    const traces =
      channel === null
        ? session.traceEngine.traces()
        : session.traceEngine.tracesForChannel(clampChannelId(channel));
    let frames = 0;
    let bytes = 0;
    let subscriptions = 0;
    let liveSubscriptions = 0;
    let malformed = 0;
    let orphaned = 0;
    let retryStorms = 0;
    let truncated = 0;
    let sensitive = 0;
    let out = 0;
    let inbound = 0;
    let durationTotal = 0;
    let durationMax = 0;
    const methodCounts = new Map<string, number>();
    for (const { view } of viewsFor(traces)) {
      frames += view.frames.length;
      durationTotal += view.durationMs;
      if (view.durationMs > durationMax) durationMax = view.durationMs;
      if (view.badges.includes("malformed")) malformed += 1;
      if (view.badges.includes("orphaned")) orphaned += 1;
      if (view.badges.includes("retry-storm")) retryStorms += 1;
      if (view.badges.includes("truncated")) truncated += 1;
      if (view.sensitive) sensitive += 1;
      if (view.frames.some((f) => SUBSCRIPTION_ROLES.has(f.role))) {
        subscriptions += 1;
        if (!view.frames.some((f) => f.role === "stop")) {
          liveSubscriptions += 1;
        }
      }
      for (const f of view.frames) {
        bytes += f.byteLength ?? 0;
        if (f.direction === "out") out += 1;
        else inbound += 1;
      }
      const opener =
        view.frames.find((f) => f.role === "request" || f.role === "start") ??
        view.frames.find((f) => f.method !== undefined);
      const method = opener?.method ?? "(unknown)";
      methodCounts.set(method, (methodCounts.get(method) ?? 0) + 1);
    }
    const ops = traces.length;
    const topMethods = [...methodCounts.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(([method, count]) => ({ method, count }));
    // Whole-op eviction (session-wide) and host-reported drops are loss the ops
    // list can't show: `ops` counts only the survivors, so without these a
    // 10k-op session that kept 256 reads as "256 ops" with no sign the rest were
    // dropped. `codecMismatch` flags a host whose wire contract differs.
    const evictedTraces = session.traceEngine.evictedTraces();
    const chanList =
      channel === null
        ? [...channels.values()]
        : [...channels.values()].filter(
            (c) => c.channelId === clampChannelId(channel),
          );
    const droppedByHost = chanList.reduce((n, c) => n + c.dropped, 0);
    const codecMismatch = chanList.some((c) => !c.codecOk);
    // Typed so a dropped/renamed field is a compile error, not a silent gap in
    // the payload the CLI parses back as CliStats.
    const payload: CliStats = {
      ops,
      frames,
      bytes,
      subscriptions,
      liveSubscriptions,
      malformed,
      orphaned,
      retryStorms,
      truncated,
      evictedTraces,
      droppedByHost,
      codecMismatch,
      sensitive,
      out,
      in: inbound,
      avgDurationMs: ops === 0 ? 0 : Math.round(durationTotal / ops),
      maxDurationMs: Math.round(durationMax),
      topMethods,
    };
    return JSON.stringify(payload);
  }

  /** The op's method for sorting: the first frame that resolves to one. */
  function traceMethod(
    trace: ReturnType<typeof session.traceEngine.traces>[number],
  ): string {
    for (const f of trace.frames) {
      const method = session.methodNames.get(f.frameId)?.method;
      if (method !== undefined) return method;
    }
    return "";
  }

  /**
   * Order the op list for the `?sort=` control. Default (`""`) keeps arrival
   * order (stable under live updates); the others are one-shot reorders the
   * client's keyed diff mirrors into the DOM.
   */
  function sortTraces(
    traces: ReturnType<typeof session.traceEngine.traces>,
    sort: string | null,
  ): ReturnType<typeof session.traceEngine.traces> {
    if (!sort) return traces;
    const copy = [...traces];
    switch (sort) {
      case "recent":
        return copy.sort((a, b) => b.lastAt - a.lastAt);
      case "duration":
        return copy.sort(
          (a, b) => b.lastAt - b.startedAt - (a.lastAt - a.startedAt),
        );
      case "frames":
        return copy.sort((a, b) => b.frames.length - a.frames.length);
      case "method":
        return copy.sort((a, b) => traceMethod(a).localeCompare(traceMethod(b)));
      default:
        return traces;
    }
  }

  /**
   * The `/op-list?channel=&sort=` primary view: one server-rendered row per op
   * (the shared {@link renderOperationRow}), payload-blind. Retry-storm is a
   * cross-op signal computed here and fed to each view as an extra badge.
   * `channel` filters on the trace's channelId; `sort` reorders the rows.
   */
  function opListHtml(channel: string | null, sort: string | null): string {
    const base =
      channel === null
        ? session.traceEngine.traces()
        : session.traceEngine.tracesForChannel(clampChannelId(channel));
    // Retry-storm is per-channel (a burst of like ops from one host), so it is
    // detected over exactly the traces being listed - before any reorder, since
    // the storm map is keyed by the trace object, not its position.
    const storms = detectRetryStorms(base);
    if (base.length === 0) {
      return `<div class="td-op-empty">no operations yet</div>`;
    }
    const rows = sortTraces(base, sort);
    // If any listed op is from a host whose wire contract differs from this
    // debugger's, its method names may be wrong. Warn inline above the rows - not
    // only in the global banner - so the mislabeled rows carry the caveat.
    // "Unreliable" = a mismatched OR merely unconfirmed host: either way its
    // method names come from this debugger's table and may be wrong, so the label
    // matches the decode gate's bar rather than the narrower banner.
    const mismatched = new Set(
      [...channels.values()]
        .filter((c) => !c.codecOk || !c.schemaOk)
        .map((c) => c.channelId),
    );
    const notice =
      mismatched.size > 0 &&
      rows.some((t) => mismatched.has(clampChannelId(t.channelId)))
        ? `<div style="padding:4px 10px;color:#fca5a5;font-size:11px;border-bottom:1px solid rgba(255,255,255,.08)">⚠ a connected host's wire contract differs from this debugger's — method names below may be wrong</div>`
        : "";
    return (
      notice + rows.map((t) => renderOperationRow(toView(t, storms))).join("")
    );
  }

  /**
   * The `/op?id=&channel=` detail fragment: the selected op via
   * {@link renderTraceDetail}. `channel` disambiguates the `requestId` when more
   * than one host is connected (each mints the same `p:N` ids).
   */
  function opDetailHtml(
    requestId: string,
    channel: string | null,
    generation?: number,
  ): string {
    const trace = session.traceEngine.trace(
      requestId,
      channel ?? undefined,
      generation,
    );
    if (!trace) {
      return `<div class="td-detail-empty">operation not found</div>`;
    }
    const storms = detectRetryStorms(
      session.traceEngine.tracesForChannel(trace.channelId),
    );
    return renderTraceDetail(toView(trace, storms), {
      offerDecode: session.decodeValues,
      offerReveal: session.revealSensitive,
    });
  }

  const server = Bun.serve({
    port: options.port ?? DEFAULT_PORT,
    // Loopback only. The debugger holds every trace (and, with decode on, decoded
    // values), so it must not listen on all interfaces where a LAN peer could
    // read them or inject frames. The CLI and same-origin inspector both target
    // localhost, so nothing else changes.
    hostname: "127.0.0.1",
    fetch(req, srv) {
      // Reject cross-origin WebSocket upgrades (CSWSH): binding to 127.0.0.1
      // keeps off-box peers out, but a page open in the dev's own browser could
      // still dial ws://127.0.0.1:<port> to inject frames or drive the decoder
      // over hostile bytes. A same-origin inspector and non-browser clients are
      // allowed; a foreign browser Origin is not.
      if (req.headers.get("upgrade")?.toLowerCase() === "websocket") {
        if (!originAllowed(req.headers.get("origin"))) {
          return new Response("forbidden origin", { status: 403 });
        }
        if (srv.upgrade(req)) return undefined;
      }
      const url = new URL(req.url);
      const htmlHeaders = { "content-type": "text/html; charset=utf-8" };
      if (url.pathname === "/traces") {
        return new Response(tracesJson(), {
          headers: { "content-type": "application/json" },
        });
      }
      if (url.pathname === "/channels") {
        return new Response(channelsJson(), {
          headers: { "content-type": "application/json" },
        });
      }
      if (url.pathname === "/stats") {
        return new Response(statsJson(url.searchParams.get("channel")), {
          headers: { "content-type": "application/json" },
        });
      }
      if (url.pathname === "/op-list") {
        return new Response(
          opListHtml(
            url.searchParams.get("channel"),
            url.searchParams.get("sort"),
          ),
          { headers: htmlHeaders },
        );
      }
      if (url.pathname === "/op") {
        const id = url.searchParams.get("id");
        const generation = optionalInt(url.searchParams.get("gen"));
        if (generation === null) {
          return new Response(`<div class="td-detail-empty">bad request</div>`, {
            status: 400,
            headers: htmlHeaders,
          });
        }
        return new Response(
          id === null
            ? `<div class="td-detail-empty">select an operation</div>`
            : opDetailHtml(id, url.searchParams.get("channel"), generation),
          { headers: htmlHeaders },
        );
      }
      if (url.pathname === "/view") {
        return new Response(viewHtml(), { headers: htmlHeaders });
      }
      if (url.pathname === "/frame") {
        return frameResponse(url);
      }
      if (url.pathname === "/frame-html") {
        return frameHtmlResponse(url);
      }
      return new Response(
        VIEW_HTML.replace("__DECODE_STATE__", decodeValues ? "on" : "off"),
        { headers: htmlHeaders },
      );
    },
    websocket: {
      open() {
        openSockets += 1;
      },
      close() {
        openSockets = Math.max(0, openSockets - 1);
      },
      message(_ws, message) {
        // Defensive: a malformed frame must never take down the socket callback.
        // parseWireMessage + the Result-based ingest don't throw today, but keep
        // the invariant local so a future ingest change can't propagate here.
        try {
          const raw = typeof message === "string" ? message : message.toString();
          const parsed = parseWireMessage(raw);
          if (parsed) {
            recordChannel(parsed.envelope.channelId, parsed);
            // Still grouped (payload-blind is safe and useful); a mismatch only
            // blocks the value-decode path, via decodeTrusted.
            session.handleEnvelope(parsed.envelope);
          }
        } catch {
          // Drop the frame; the observed session is worth more than one trace.
        }
      },
    },
  });

  return {
    // Always a TCP port here; the `?? 0` only satisfies Bun's unix-socket union.
    port: server.port ?? 0,
    decodeValues,
    revealSensitive,
    stop: () => server.stop(true),
  };
}

/**
 * The wire inspector: a full-screen, host-agnostic dev tool - a Network tab for
 * TrUAPI wire frames. Left is the operation list (one row per op, the primary
 * view); right is the selected op's frame sequence via the shared
 * {@link renderTraceDetail}. A top bar switches between the hosts that have
 * dialed in; a status bar shows counts and liveness.
 *
 * The client is a thin shell over server-rendered fragments: it polls
 * `/op-list` (the shared {@link renderOperationRow}) and `/channels`, and fetches
 * `/op` and `/frame-html` on interaction. Every injected fragment is produced
 * and escaped server-side, so `innerHTML` is safe. Payload-blind by default:
 * `/op-list` and `/op` carry only shape/timing; a value appears only after an
 * explicit per-frame decode, and a sensitive frame renders redacted, never its
 * value. `td-*` classes are owned by the shared renderer.
 */
const VIEW_HTML = `<!doctype html>
<meta charset="utf-8">
<title>TrUAPI Wire Inspector</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  html, body { height: 100%; margin: 0; }
  body { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace;
    background: #0a0a0a; color: #e0e0e0; display: grid;
    grid-template-rows: auto auto 1fr auto; height: 100vh; overflow: hidden; }
  .ins-top { display: flex; align-items: center; gap: 12px; padding: 6px 12px;
    border-bottom: 1px solid rgba(255,255,255,.08); }
  .ins-title { font-weight: 600; letter-spacing: .02em; white-space: nowrap; }
  .ins-title .accent { color: #4ade80; }
  .ins-channels { display: flex; gap: 6px; flex: 1; flex-wrap: wrap; }
  .ins-chan { display: inline-flex; align-items: center; gap: 5px; padding: 1px 9px;
    border: 1px solid rgba(255,255,255,.12); border-radius: 10px; background: transparent;
    color: #94a3b8; cursor: pointer; font: inherit; }
  .ins-chan.active { color: #0a0a0a; background: #4ade80; border-color: #4ade80; }
  .ins-chan .dot { width: 6px; height: 6px; border-radius: 50%; background: #4b5563; }
  .ins-chan .dot.live { background: #4ade80; box-shadow: 0 0 4px #4ade80; }
  .ins-chan.active .dot.live { background: #0a0a0a; box-shadow: none; }
  .ins-gate { color: #6b7280; white-space: nowrap; }
  .ins-gate.on { color: #fbbf24; }
  .ins-body { display: grid; grid-template-columns: var(--list-w, 340px) 6px 1fr;
    min-height: 0; }
  .ins-list { overflow: auto; outline: none; }
  .ins-split { cursor: col-resize; background: rgba(255,255,255,.05); }
  .ins-split:hover { background: rgba(74,222,128,.4); }
  .ins-detail { overflow: auto; padding: 8px 12px; outline: none; }
  .td-op { display: flex; align-items: center; gap: 8px; padding: 4px 10px;
    cursor: pointer; border-bottom: 1px solid rgba(255,255,255,.03); }
  .td-op:hover { background: rgba(255,255,255,.04); }
  .td-op.selected { background: rgba(74,222,128,.13); }
  .ins-list:focus-visible .td-op.selected { box-shadow: inset 2px 0 0 #4ade80; }
  .td-op-kind { width: 12px; text-align: center; }
  .td-op-req .td-op-kind { color: #fbbf24; }
  .td-op-sub .td-op-kind { color: #c084fc; }
  .td-op-method { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .td-op-method.anon { color: #525252; font-style: italic; }
  .td-op-meta { color: #6b7280; font-size: 10.5px; white-space: nowrap; }
  .td-op-live .td-op-meta { color: #4ade80; }
  .td-op-badges { display: inline-flex; gap: 4px; }
  .td-op-empty, .td-detail-empty { color: #6b7280; padding: 14px; }
  .td-frame.cursor { background: rgba(255,255,255,.06); box-shadow: inset 2px 0 0 #94a3b8; }
${TRACE_DETAIL_CSS}
  /* App-level layout for the drill-down (trace-styles.ts stays untouched).
     Each frame is a two-column grid: meta on the left, a fixed-width payload
     column on the right, so every frame's decoded / blurred box opens in the
     same aligned partitioned space instead of trailing variable-width meta. */
  .ins-detail { padding: 6px 10px 10px; }
  .td-frame { display: grid; align-items: start; column-gap: 10px;
    grid-template-columns: minmax(0, 1fr); padding: 4px 8px; }
  .td-frame:has(.td-frame-payload) {
    grid-template-columns: minmax(0, 1fr) var(--payload-w, clamp(240px, 44%, 520px)); }
  .td-frame-meta { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .td-frame-meta .td-frame-method { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .td-frames .td-frame:nth-child(even) { background: rgba(255,255,255,.02); }
  .td-frame:hover { background: rgba(255,255,255,.05); }
  /* The payload column: same width for every frame; content scrolls inside. */
  .td-frame-payload { min-width: 0; }
  .td-frame-decoded > * { margin: 0; }
  .td-frame-decoded .td-detail-pre { max-height: 240px; overflow: auto; margin: 0;
    white-space: pre; }
  /* Blur-to-reveal placeholder: decorative blocks (no real bytes), revealed on
     decode. Full width of the payload column so all placeholders line up. */
  .td-frame-decode-btn { display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 3px 8px; border: 1px solid rgba(255,255,255,.10); border-radius: 5px;
    background: rgba(255,255,255,.03); color: #94a3b8; cursor: pointer;
    font: inherit; text-align: left; transition: background .12s, border-color .12s; }
  .td-frame-decode-btn:hover { background: rgba(74,222,128,.10); border-color: rgba(74,222,128,.4); color: #d1fae5; }
  .td-frame-decode-btn:disabled { opacity: .5; cursor: progress; }
  .td-enc-blur { flex: 1; min-width: 0; overflow: hidden; color: #64748b;
    filter: blur(3px); user-select: none; letter-spacing: -1px; }
  .td-enc-hint { white-space: nowrap; font-size: 10.5px; color: #6b7280; }
  .td-frame-decode-btn:hover .td-enc-hint { color: #86efac; }
  /* Bulk decode/encode controls in the top bar (shown only when decode is on). */
  .ins-bulk { display: none; gap: 6px; }
  .ins-bulk.on { display: inline-flex; }
  .ins-btn { padding: 1px 9px; border: 1px solid rgba(255,255,255,.14); border-radius: 5px;
    background: transparent; color: #cbd5e1; cursor: pointer; font: inherit; white-space: nowrap; }
  .ins-btn:hover { border-color: rgba(74,222,128,.5); color: #86efac; }
  .ins-btn.primary { border-color: rgba(251,191,36,.45); color: #fbbf24; }
  .ins-btn.primary:hover { background: rgba(251,191,36,.12); }
  /* Top-bar filter / sort / sensitive-only controls. */
  .ins-filter { width: 148px; padding: 2px 8px; border: 1px solid rgba(255,255,255,.14);
    border-radius: 5px; background: rgba(255,255,255,.03); color: #e0e0e0; font: inherit; }
  .ins-filter:focus { outline: none; border-color: rgba(74,222,128,.5); }
  .ins-sort { padding: 2px 6px; border: 1px solid rgba(255,255,255,.14); border-radius: 5px;
    background: #0a0a0a; color: #cbd5e1; font: inherit; cursor: pointer; }
  .ins-sens-toggle.active { border-color: #f87171; color: #f87171; background: rgba(248,113,113,.10); }
  .td-op.filtered-out { display: none; }
  /* Privacy markers on the op row and the frame. */
  .td-op-lock, .td-frame-lock { font-size: 10px; opacity: .9; }
  .td-op-lock { margin-left: 3px; }
  .td-frame-lock { margin-left: -3px; }
  .ins-stat.lock .n { color: #fca5a5; }
  /* Clickable top-method pills. */
  .ins-method { cursor: pointer; }
  .ins-method:hover { border-color: rgba(74,222,128,.5); color: #d1fae5; }
  /* Sensitive-reveal escape hatch (dev-only, env-armed): danger styling. */
  .td-frame-reveal-btn { display: flex; align-items: center; gap: 6px; width: 100%;
    padding: 3px 8px; border: 1px dashed rgba(248,113,113,.55); border-radius: 5px;
    background: rgba(248,113,113,.06); color: #f87171; cursor: pointer; font: inherit; text-align: left; }
  .td-frame-reveal-btn:hover { background: rgba(248,113,113,.15); border-style: solid; }
  .td-detail-danger { border-color: rgba(248,113,113,.6) !important;
    box-shadow: inset 3px 0 0 #f87171; }
  /* Aggregate summary strip: the "at a glance" row of metric tiles. */
  .ins-summary { display: flex; gap: 6px; align-items: flex-start; flex-wrap: nowrap;
    padding: 6px 12px; border-bottom: 1px solid rgba(255,255,255,.08);
    background: rgba(255,255,255,.02); overflow-x: auto; }
  .ins-stat { display: flex; flex-direction: column; gap: 1px; padding: 2px 10px 2px 0;
    border-right: 1px solid rgba(255,255,255,.06); }
  .ins-stat:last-child { border-right: 0; }
  .ins-stat .n { font-size: 14px; font-weight: 600; color: #f1f5f9;
    font-variant-numeric: tabular-nums; line-height: 1.15; }
  .ins-stat .k { font-size: 9.5px; text-transform: uppercase; letter-spacing: .06em; color: #64748b; }
  .ins-stat.warn .n { color: #f87171; }
  .ins-stat.warn.zero .n { color: #475569; }
  .ins-stat.good .n { color: #4ade80; }
  .ins-stat .sub { color: #64748b; font-weight: 400; font-size: 10px; }
  /* Pills stay on one row, pushed right; when the viewport is too narrow the
     whole summary scrolls (overflow-x above) rather than the pills wrapping to a
     second line. */
  .ins-methods { display: flex; align-items: center; gap: 6px; margin-left: auto;
    flex: 0 0 auto; flex-wrap: nowrap; }
  .ins-method { white-space: nowrap; }
  .ins-method { display: inline-flex; align-items: center; gap: 5px; padding: 1px 8px;
    border: 1px solid rgba(255,255,255,.08); border-radius: 10px; color: #94a3b8;
    font-size: 10.5px; white-space: nowrap; }
  .ins-method b { color: #cbd5e1; font-variant-numeric: tabular-nums; }
  .ins-summary.empty { color: #64748b; }
  .ins-status { display: flex; gap: 16px; padding: 4px 12px; color: #6b7280;
    border-top: 1px solid rgba(255,255,255,.08); }
  .ins-status .live { color: #4ade80; }
  .ins-status .mismatch { color: #f87171; }
</style>
<div class="ins-top">
  <span class="ins-title">TrUAPI <span class="accent">Wire Inspector</span></span>
  <input class="ins-filter" id="filter" type="search" placeholder="filter methods…" autocomplete="off" spellcheck="false">
  <select class="ins-sort" id="sort" title="Sort operations">
    <option value="">arrival</option>
    <option value="recent">recent</option>
    <option value="method">method</option>
    <option value="duration">slowest</option>
    <option value="frames">most frames</option>
  </select>
  <button class="ins-btn ins-sens-toggle" id="sensOnly" type="button"
    title="Show only ops carrying a sensitive (redacted) method">🔒 only</button>
  <span class="ins-channels" id="channels"></span>
  <span class="ins-bulk" id="bulk">
    <button class="ins-btn primary" id="decodeAll" type="button"
      title="Reveal every non-sensitive payload in the open op (sensitive frames stay redacted)">Decode all</button>
    <button class="ins-btn" id="encodeAll" type="button"
      title="Re-blur every payload in the open op">Encode all</button>
  </span>
  <span class="ins-gate" id="gate">decode: __DECODE_STATE__</span>
</div>
<div class="ins-summary empty" id="summary">waiting for frames…</div>
<div class="ins-body">
  <div class="ins-list" id="list" tabindex="0"><div class="td-op-empty">waiting for frames…</div></div>
  <div class="ins-split" id="split" title="Drag to resize"></div>
  <div class="ins-detail" id="detail" tabindex="0"><div class="td-detail-empty">Select an operation to inspect its frames. ↑/↓ to move, Enter to open, d to decode a frame.</div></div>
</div>
<div class="ins-status" id="status">connecting…</div>
<script>
  var listEl = document.getElementById("list");
  var detailEl = document.getElementById("detail");
  var chanEl = document.getElementById("channels");
  var statusEl = document.getElementById("status");
  var summaryEl = document.getElementById("summary");
  var gateEl = document.getElementById("gate");
  var bulkEl = document.getElementById("bulk");
  var filterEl = document.getElementById("filter");
  var sortEl = document.getElementById("sort");
  var sensOnlyEl = document.getElementById("sensOnly");
  var decodeEnabled = gateEl.textContent.indexOf("on") !== -1;
  if (decodeEnabled) { gateEl.classList.add("on"); bulkEl.classList.add("on"); }

  var selectedId = null;      // requestId of the open op
  var selectedChannel = null; // channelId of the open op (disambiguates requestId across hosts)
  var selectedGen = null;     // generation of the open op (disambiguates a recycled requestId)
  var channel = null;         // channelId filter, null = all
  var lastListHtml = "";   // skip rebuilds when the op list is unchanged
  var lastDetailHtml = ""; // skip detail refresh when the open op is unchanged
  var cursor = -1;         // frame index highlighted in the detail
  var decodeAll = false;   // sticky "decode every payload" mode (Decode all)
  var filter = "";         // method substring filter (client-side, over the op list)
  var sortMode = "";       // server-side sort key ("" = arrival order)
  var sensOnly = false;    // show only ops carrying a sensitive method

  // The op list is a live, keyed-diff DOM; filtering hides rows with a class and
  // must be re-applied after every rebuild. Sort is a server concern (?sort=).
  function applyFilter() {
    rows().forEach(function (r) {
      var m = r.querySelector(".td-op-method");
      var text = m ? m.textContent.toLowerCase() : "";
      var hideText = filter !== "" && text.indexOf(filter) === -1;
      var hideSens = sensOnly && !r.hasAttribute("data-sensitive");
      r.classList.toggle("filtered-out", hideText || hideSens);
    });
  }
  filterEl.addEventListener("input", function () {
    filter = filterEl.value.trim().toLowerCase();
    applyFilter();
  });
  sortEl.addEventListener("change", function () {
    sortMode = sortEl.value;
    lastListHtml = "";  // force a rebuild under the new order
    poll();
  });
  sensOnlyEl.addEventListener("click", function () {
    sensOnly = !sensOnly;
    sensOnlyEl.classList.toggle("active", sensOnly);
    applyFilter();
  });
  // Click a top-method pill to filter to it.
  summaryEl.addEventListener("click", function (e) {
    var pill = e.target.closest && e.target.closest(".ins-method");
    if (!pill) return;
    filter = (pill.getAttribute("data-method") || "").toLowerCase();
    filterEl.value = pill.getAttribute("data-method") || "";
    applyFilter();
  });

  function get(url) { return fetch(url).then(function (r) { return r.text(); }); }

  function keyOf(el) {
    return el.getAttribute("data-request-id") + "\\0" + (el.getAttribute("data-channel-id") || "") + "\\0" + (el.getAttribute("data-generation") || "0");
  }
  // The selected op's identity is (requestId, channelId), not requestId alone -
  // two hosts on the "all" view mint the same p:N, so selection, the keyed diff,
  // and keyboard nav must all match on the composite key.
  function selKey() {
    return selectedId === null ? null : selectedId + "\\0" + (selectedChannel || "") + "\\0" + (selectedGen || "0");
  }
  function visibleRows() {
    return rows().filter(function (r) { return !r.classList.contains("filtered-out"); });
  }

  // Keyed diff of the op list: only add/remove/patch rows that actually changed,
  // keyed by (requestId, channelId). Unchanged rows keep their DOM identity, so
  // selection, keyboard focus, and an in-flight click survive a live update -
  // critical because a live subscription's row changes every poll.
  function applyList(html) {
    if (html === lastListHtml) return;
    lastListHtml = html;
    var tmp = document.createElement("div");
    tmp.innerHTML = html;
    var incoming = Array.prototype.slice.call(tmp.children);
    // Empty / waiting state (a single non-row div): replace wholesale.
    if (incoming.length === 0 || incoming[0].getAttribute("data-request-id") === null) {
      listEl.innerHTML = html;
      return;
    }
    // Drop any leftover placeholder (e.g. the initial "waiting" state) before
    // the first real rows land.
    Array.prototype.slice.call(listEl.children).forEach(function (c) {
      if (!c.classList || !c.classList.contains("td-op")) c.remove();
    });
    var existing = {};
    Array.prototype.slice.call(listEl.querySelectorAll(".td-op")).forEach(function (r) {
      existing[keyOf(r)] = r;
    });
    var seen = {};
    var prev = null;
    incoming.forEach(function (row) {
      var key = keyOf(row);
      seen[key] = true;
      var cur = existing[key];
      if (cur) {
        if (cur.innerHTML !== row.innerHTML) cur.innerHTML = row.innerHTML;
        if (cur.className !== row.className) cur.className = row.className;
      } else {
        cur = row;
      }
      cur.classList.toggle("selected", selKey() !== null && keyOf(cur) === selKey());
      // Place in incoming order without disturbing untouched neighbours.
      var want = prev ? prev.nextSibling : listEl.firstChild;
      if (cur !== want) listEl.insertBefore(cur, want);
      prev = cur;
    });
    Array.prototype.slice.call(listEl.querySelectorAll(".td-op")).forEach(function (r) {
      if (!seen[keyOf(r)]) r.remove();
    });
    applyFilter();  // rows changed; re-apply the active filter/sensitive view
  }

  function rows() { return Array.prototype.slice.call(listEl.querySelectorAll(".td-op")); }

  function selectOp(id, chan, gen) {
    selectedId = id;
    selectedChannel = chan || null;
    selectedGen = gen == null ? "0" : String(gen);
    var want = selKey();
    var row = null;
    rows().forEach(function (r) {
      var on = keyOf(r) === want;
      r.classList.toggle("selected", on);
      if (on) row = r;
    });
    cursor = -1;
    get("/op?id=" + encodeURIComponent(id) +
      (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
      "&gen=" + encodeURIComponent(selectedGen))
      .then(function (frag) {
        lastDetailHtml = frag;
        detailEl.innerHTML = frag;
        // Sticky Decode-all: a freshly opened op reveals every payload too, so
        // the mode persists as the user moves between ops.
        if (decodeAll) decodeAllFrames();
      });
    if (row) row.scrollIntoView({ block: "nearest" });
  }

  // List keyboard: step selection, open detail.
  listEl.addEventListener("keydown", function (e) {
    // Navigate only the rows the current filter actually shows.
    var rs = visibleRows();
    if (rs.length === 0) return;
    var key = selKey();
    var idx = rs.findIndex(function (r) { return keyOf(r) === key; });
    function pick(r) { selectOp(r.getAttribute("data-request-id"), r.getAttribute("data-channel-id"), r.getAttribute("data-generation")); }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      var n = idx < 0 ? 0 : Math.min(idx + 1, rs.length - 1);
      pick(rs[n]);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      var p = idx < 0 ? rs.length - 1 : Math.max(idx - 1, 0);
      pick(rs[p]);
    } else if (e.key === "Enter" || e.key === "ArrowRight") {
      e.preventDefault();
      detailEl.focus();
      moveCursor(0);
    } else if (e.key === "Escape") {
      selectedId = null;
      selectedChannel = null;
      lastDetailHtml = "";
      rows().forEach(function (r) { r.classList.remove("selected"); });
      detailEl.innerHTML = '<div class="td-detail-empty">Select an operation to inspect its frames.</div>';
    }
  });
  listEl.addEventListener("click", function (e) {
    var row = e.target.closest && e.target.closest(".td-op");
    if (row) { listEl.focus(); selectOp(row.getAttribute("data-request-id"), row.getAttribute("data-channel-id"), row.getAttribute("data-generation")); }
  });

  // Detail keyboard: move a frame cursor, decode the cursored frame.
  function frameEls() { return Array.prototype.slice.call(detailEl.querySelectorAll(".td-frame")); }
  function moveCursor(next) {
    var fs = frameEls();
    if (fs.length === 0) return;
    cursor = Math.max(0, Math.min(next, fs.length - 1));
    fs.forEach(function (f, i) { f.classList.toggle("cursor", i === cursor); });
    fs[cursor].scrollIntoView({ block: "nearest" });
  }
  detailEl.addEventListener("keydown", function (e) {
    if (e.key === "ArrowDown") { e.preventDefault(); moveCursor(cursor + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); moveCursor(cursor - 1); }
    else if (e.key === "ArrowLeft" || e.key === "Escape") { e.preventDefault(); listEl.focus(); }
    else if (e.key === "d" || e.key === "Enter") {
      var fs = frameEls();
      if (cursor >= 0 && fs[cursor]) {
        var btn = fs[cursor].querySelector(".td-frame-decode-btn");
        if (btn) decodeFrame(btn);
      }
    }
  });

  // Level-2: fetch the /frame-html fragment and swap it in for the decode
  // control. Server-rendered + escaped; a sensitive frame comes back redacted.
  function decodeFrame(btn) {
    var trace = btn.closest(".td-trace");
    var id = trace && trace.getAttribute("data-request-id");
    var seq = btn.getAttribute("data-seq");
    if (!id || seq === null) return;
    btn.disabled = true;
    get("/frame-html?id=" + encodeURIComponent(id) + "&i=" + encodeURIComponent(seq) +
      (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
      "&gen=" + encodeURIComponent(selectedGen || "0"))
      .then(function (frag) { btn.outerHTML = frag; })
      .catch(function () { btn.disabled = false; });
  }
  detailEl.addEventListener("click", function (e) {
    var btn = e.target.closest && e.target.closest(".td-frame-decode-btn");
    if (btn) { decodeFrame(btn); return; }
    // Sensitive-reveal escape hatch: explicit per-frame confirm before we ask
    // the server (which only honors it when the reveal gate is armed).
    var rb = e.target.closest && e.target.closest(".td-frame-reveal-btn");
    if (rb) revealFrame(rb);
  });

  // Reveal one sensitive frame after an explicit confirmation. Distinct from
  // decodeFrame: it passes reveal=1 and is never swept up by "Decode all".
  function revealFrame(btn) {
    if (!window.confirm(
      "Reveal this SENSITIVE payload?\\n\\nIt may contain a private key, signature, or credential. " +
      "Do NOT do this while screen-sharing or recording."
    )) return;
    var trace = btn.closest(".td-trace");
    var id = trace && trace.getAttribute("data-request-id");
    var seq = btn.getAttribute("data-seq");
    if (!id || seq === null) return;
    btn.disabled = true;
    get("/frame-html?id=" + encodeURIComponent(id) + "&i=" + encodeURIComponent(seq) + "&reveal=1" +
      (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
      "&gen=" + encodeURIComponent(selectedGen || "0"))
      .then(function (frag) { btn.outerHTML = frag; })
      .catch(function () { btn.disabled = false; });
  }

  // Bulk controls: decode / re-blur every payload in the open op at once, so a
  // reviewer never chases one button per frame. Decode-all is server-gated and
  // denylist-safe like the per-frame path - a sensitive frame still comes back
  // redacted, never its value.
  function decodeAllFrames() {
    Array.prototype.slice
      .call(detailEl.querySelectorAll(".td-frame-decode-btn"))
      .forEach(function (b) { if (!b.disabled) decodeFrame(b); });
  }
  function encodeAllFrames() {
    if (!selectedId) return;
    // Re-render the op from /op: every payload returns to its blurred
    // placeholder (the server offers controls, not values).
    cursor = -1;  // the re-render clears .cursor; keep the index in step
    get("/op?id=" + encodeURIComponent(selectedId) +
      (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
      "&gen=" + encodeURIComponent(selectedGen || "0"))
      .then(function (frag) { detailEl.innerHTML = frag; });
  }
  var decodeAllBtn = document.getElementById("decodeAll");
  var encodeAllBtn = document.getElementById("encodeAll");
  if (decodeAllBtn) decodeAllBtn.addEventListener("click", function () {
    decodeAll = true; decodeAllFrames();
  });
  if (encodeAllBtn) encodeAllBtn.addEventListener("click", function () {
    decodeAll = false; encodeAllFrames();
  });

  // Channel switcher + status, from /channels connection state. Liveness uses
  // the server's connected flag (one recency threshold, server-side) rather
  // than recomputing here, so the dot and the server agree.
  function renderChannels(data) {
    var live = 0;
    var html = '<button class="ins-chan' + (channel === null ? " active" : "") +
      '" data-chan="">all</button>';
    (data.channels || []).forEach(function (c) {
      if (c.connected) live++;
      html += '<button class="ins-chan' + (channel === c.channelId ? " active" : "") +
        '" data-chan="' + encodeURIComponent(c.channelId) + '">' +
        '<span class="dot' + (c.connected ? " live" : "") + '"></span>' + escHtml(c.channelId) + "</button>";
    });
    chanEl.innerHTML = html;
    var hosts = (data.channels || []).length;
    // A host streaming a wire codec this debugger can't decode against: value
    // decode is refused for it (payload-blind grouping still works). Banner it.
    var codecWarn = data.codecMismatch
      ? ' · <span class="mismatch" title="A host is streaming a wire codec this debugger cannot decode against; value decode is refused for it.">⚠ codec mismatch</span>'
      : "";
    statusEl.innerHTML = rows().length + " ops · " + hosts + " host" + (hosts === 1 ? "" : "s") +
      " · " + (live > 0 ? '<span class="live">' + live + " live</span>" : "idle") + codecWarn;
  }
  function escHtml(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  // Aggregate summary strip: the at-a-glance roll-up from /stats (payload-blind -
  // counts, bytes, durations, health, direction split, busiest methods).
  function fmtBytes(n) {
    if (n < 1024) return n + " B";
    if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB";
    return (n / (1024 * 1024)).toFixed(2) + " MB";
  }
  function fmtMs(ms) {
    return ms < 1000 ? Math.round(ms) + "ms" : (ms / 1000).toFixed(2) + "s";
  }
  function statTile(n, k, sub) {
    return '<div class="ins-stat"><span class="n">' + escHtml(String(n)) +
      (sub ? ' <span class="sub">' + escHtml(sub) + "</span>" : "") +
      '</span><span class="k">' + escHtml(k) + "</span></div>";
  }
  function warnTile(n, k) {
    return '<div class="ins-stat warn' + (n === 0 ? " zero" : "") +
      '"><span class="n">' + n + '</span><span class="k">' + escHtml(k) + "</span></div>";
  }
  function renderStats(s) {
    if (!s || !s.ops) {
      summaryEl.className = "ins-summary empty";
      summaryEl.textContent = "waiting for frames…";
      return;
    }
    summaryEl.className = "ins-summary";
    var html = statTile(s.ops, "ops") +
      statTile(s.frames, "frames", s.out + "▶ " + s["in"] + "◀") +
      statTile(fmtBytes(s.bytes), "data") +
      statTile(s.subscriptions, "subs", s.liveSubscriptions > 0 ? s.liveSubscriptions + " live" : "") +
      statTile(fmtMs(s.avgDurationMs), "avg op", "max " + fmtMs(s.maxDurationMs) + ", observed") +
      '<div class="ins-stat lock"><span class="n">' + (s.sensitive || 0) +
        '</span><span class="k">🔒 sensitive</span></div>' +
      warnTile(s.malformed, "malformed") +
      warnTile(s.orphaned, "orphaned") +
      warnTile(s.retryStorms, "retry storms") +
      warnTile(s.truncated || 0, "truncated") +
      warnTile(s.evictedTraces || 0, "evicted") +
      warnTile(s.droppedByHost || 0, "dropped");
    if (s.topMethods && s.topMethods.length) {
      var m = '<div class="ins-methods">';
      s.topMethods.forEach(function (t) {
        m += '<span class="ins-method" data-method="' + escHtml(t.method) + '">' +
          escHtml(t.method) + " <b>" + t.count + "</b></span>";
      });
      html += m + "</div>";
    }
    summaryEl.innerHTML = html;
  }
  chanEl.addEventListener("click", function (e) {
    var btn = e.target.closest && e.target.closest(".ins-chan");
    if (!btn) return;
    var c = btn.getAttribute("data-chan");
    channel = c === "" ? null : decodeURIComponent(c);
    lastListHtml = "";   // force a rebuild under the new filter
    poll();
  });

  // Splitter drag.
  var dragging = false;
  document.getElementById("split").addEventListener("pointerdown", function (e) {
    dragging = true; e.target.setPointerCapture(e.pointerId);
    document.body.style.userSelect = "none";
  });
  window.addEventListener("pointermove", function (e) {
    if (!dragging) return;
    var w = Math.max(220, Math.min(e.clientX, window.innerWidth - 320));
    document.body.style.setProperty("--list-w", w + "px");
  });
  window.addEventListener("pointerup", function () {
    dragging = false; document.body.style.userSelect = "";
  });

  function poll() {
    var base = channel === null ? [] : ["channel=" + encodeURIComponent(channel)];
    var q = base.length ? "?" + base.join("&") : "";
    var listP = sortMode ? base.concat("sort=" + encodeURIComponent(sortMode)) : base;
    var listQ = listP.length ? "?" + listP.join("&") : "";
    get("/op-list" + listQ).then(applyList).catch(function () {});
    fetch("/channels").then(function (r) { return r.json(); }).then(renderChannels).catch(function () {});
    fetch("/stats" + q).then(function (r) { return r.json(); }).then(renderStats).catch(function () {});
    // Keep the open op's detail live (a subscription gains receive frames while
    // it stays selected). Re-render only when the fragment actually changed, so
    // a quiet op keeps its in-place decode/reveal state instead of being wiped
    // every second; sticky Decode-all re-applies when it does change.
    if (selectedId) {
      get("/op?id=" + encodeURIComponent(selectedId) +
        (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
        "&gen=" + encodeURIComponent(selectedGen || "0"))
        .then(function (frag) {
          if (frag === lastDetailHtml) return;
          lastDetailHtml = frag;
          detailEl.innerHTML = frag;
          cursor = -1;
          if (decodeAll) decodeAllFrames();
        }).catch(function () {});
    }
  }
  setInterval(poll, 1000);
  poll();
</script>
`;

// Entry point: `bun run src/server.ts` (or `npm run serve`) starts the server.
// Port comes from TRUAPI_DEBUGGER_PORT, else the default. Level-2 value decode
// is off unless TRUAPI_DEBUGGER_DECODE_VALUES is truthy (1/true/yes/on).
if (import.meta.main) {
  const envPort = Number(Bun.env.TRUAPI_DEBUGGER_PORT);
  const decodeValues = /^(1|true|yes|on)$/i.test(
    Bun.env.TRUAPI_DEBUGGER_DECODE_VALUES ?? "",
  );
  const revealSensitive = /^(1|true|yes|on)$/i.test(
    Bun.env.TRUAPI_DEBUGGER_REVEAL_SENSITIVE ?? "",
  );
  const server = startDebugServer({
    port: Number.isFinite(envPort) && envPort > 0 ? envPort : DEFAULT_PORT,
    decodeValues,
    revealSensitive,
  });
  console.log(
    `[truapi-debugger] listening on http://localhost:${server.port}` +
      ` (value decode: ${server.decodeValues ? "on" : "off"}` +
      `${server.revealSensitive ? ", sensitive reveal: ARMED" : ""})`,
  );
}
