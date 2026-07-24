import {
  createMethodNameMap,
  type WireMethodInfo,
} from "./debug.js";
import {
  createTransport,
  type FrameDirection,
  type ObservedFrame,
  type TransportObserver,
} from "./client.js";
import {
  decodeWireMessage,
  encodeWireMessage,
  type WireProvider,
  type RequestFrameIds,
  type SubscriptionFrameIds,
} from "./transport.js";
import { createClient } from "./generated/client.js";
import * as W from "./generated/wire-table.js";

/** A provider that sends and receives nothing; used to enumerate the client's service namespaces. */
const NOOP_PROVIDER: WireProvider = {
  postMessage() {},
  subscribe() {
    return () => {};
  },
  dispose() {},
};

/**
 * Service namespaces of the generated client, derived from the client
 * itself so the list tracks codegen instead of a hand-maintained array.
 * `createMethodNameMap` uses it to resolve `frameId → "service.method"`.
 **/
const SERVICE_NAMES: readonly string[] = Object.keys(
  createClient(createTransport(NOOP_PROVIDER)),
);

/**
 * Per-call context handed to every mock handler. Carries the wire
 * `requestId` so a handler can correlate with the product span and the
 * wire trace observing the same operation.
 **/
export interface DebugCallContext {
  /** Transport-assigned request id of the originating client frame. */
  readonly requestId: string;
}

/**
 * Byte port a mock subscription entry uses to push receive and interrupt
 * frames back to the client.
 **/
export interface DebugSubscriptionPort {
  /** Emit a receive frame carrying the supplied encoded item bytes. */
  sendReceive(payload: Uint8Array): void;
  /**
   * Emit an interrupt frame carrying the supplied encoded reason bytes
   * and end the subscription.
   **/
  sendInterrupt(payload: Uint8Array): void;
  /** `true` once the subscription has ended. */
  readonly isClosed: boolean;
}

/** Cleanup returned by a mock subscription entry's `start`. */
export type DebugSubscriptionCleanup = () => void;

/**
 * Mock handler for a one-shot request method. `handle` receives the
 * encoded request payload and returns the encoded response payload —
 * encode both with the generated codecs so a mock cannot put a malformed
 * frame on the wire.
 **/
export interface DebugRequestEntry {
  readonly kind: "request";
  readonly ids: RequestFrameIds;
  handle(
    ctx: DebugCallContext,
    payload: Uint8Array,
  ): Uint8Array | Promise<Uint8Array>;
}

/**
 * Mock handler for a subscription method. `start` receives the encoded
 * start payload and a port for pushing items; it returns a cleanup that
 * runs when the client stops the subscription or the host disposes.
 **/
export interface DebugSubscriptionEntry {
  readonly kind: "subscription";
  readonly ids: SubscriptionFrameIds;
  start(
    ctx: DebugCallContext,
    payload: Uint8Array,
    port: DebugSubscriptionPort,
  ): DebugSubscriptionCleanup | Promise<DebugSubscriptionCleanup>;
}

/** A mock dispatch entry: request or subscription. */
export type DebugHostEntry = DebugRequestEntry | DebugSubscriptionEntry;

/**
 * Which engine a frame was routed to. `mock` frames are answered by the
 * local entries, `forward` frames travel the pipe to the real host (and
 * its answers relay back), `unhandled` frames matched no entry and no
 * forward pipe exists — the loud headless-mode failure.
 **/
export type DebugHostTier = "mock" | "forward" | "unhandled";

/**
 * One routing decision, emitted per frame in both directions. This is how
 * mocked traffic is *marked*: every frame answered by a mock entry
 * arrives here with `tier: "mock"`, so no consumer can mistake scripted
 * responses for real host behaviour.
 **/
export interface DebugHostDecision {
  /** Engine that handled (or refused) this frame. */
  tier: DebugHostTier;
  /** Dotted method path resolved from the wire table, when the id is known. */
  method?: string;
  /** The payload-blind frame the decision applies to, host vantage. */
  frame: ObservedFrame;
}

/**
 * Configuration for `createDebugHost`.
 **/
export interface CreateDebugHostOptions {
  /**
   * Product-facing provider: the pipe the client transport is connected
   * to (in-memory pair, iframe port, or a relay provider).
   **/
  provider: WireProvider;

  /**
   * Mock entries. An entry claims its wire ids: its request (or
   * start/stop) frames dispatch locally instead of down the forward pipe.
   **/
  entries?: readonly DebugHostEntry[];

  /**
   * Optional pipe to a real host — including a WASM-backed mock host.
   * Frames not claimed by an entry are forwarded verbatim — bytes
   * untouched, `requestId` untouched — and the host's answers relay back
   * the same way, so end-to-end correlation holds across the hop. Without
   * a forward pipe the debug host is a pure headless responder and
   * unclaimed frames surface as `unhandled`.
   **/
  forward?: WireProvider;

  /**
   * Payload-blind observer of every frame crossing the debug host, in
   * both directions — same `ObservedFrame` shape as the client transport
   * seam, host vantage (`in` = from the product, `out` = to the product).
   **/
  observe?: TransportObserver;

  /**
   * Routing-decision listener: one call per observed frame, carrying the
   * mock/forward/unhandled tier and the resolved method name. Isolated
   * like an observer — a throwing listener never breaks routing. When
   * unset, `unhandled` frames log a console warning instead, so headless
   * mode is loud by default.
   **/
  onDecision?: (decision: DebugHostDecision) => void;
}

/**
 * Handle returned by `createDebugHost`.
 **/
export interface DebugHost {
  /**
   * Detach from both providers, tear down live mock subscriptions, and
   * send stop frames upstream for every live forwarded subscription so
   * the real host does not keep streaming into a detached pipe. Does not
   * dispose the providers themselves.
   **/
  dispose(): void;
}

interface MockSubscriptionSlot {
  port: DebugSubscriptionPort & { close(): void };
  cleanup?: DebugSubscriptionCleanup;
  stopped: boolean;
}

/**
 * Headless debug host: mock at the dispatcher, not the frame.
 *
 * A frame router splits inbound product traffic by wire id: ids claimed
 * by a mock entry dispatch locally (payloads encoded/decoded by the
 * caller with the generated codecs); everything else forwards verbatim
 * down the `forward` pipe (when present) with answers relayed back, or is
 * surfaced loudly as `unhandled` (headless mode). The wire `requestId` is
 * never rewritten on any path, so product spans, client wire traces, and
 * the real host stay correlated under one id whether a frame was mocked
 * or forwarded.
 **/
export function createDebugHost(options: CreateDebugHostOptions): DebugHost {
  const { provider, entries = [], forward, observe, onDecision } = options;

  const wireIndex: ReadonlyMap<number, WireMethodInfo> = createMethodNameMap(
    W as unknown as Record<string, unknown>,
    SERVICE_NAMES,
  );

  // Index mock entries by their inbound wire ids; reject collisions.
  const requestEntries = new Map<number, DebugRequestEntry>();
  const subscriptionEntries = new Map<number, DebugSubscriptionEntry>();
  const stopToStart = new Map<number, number>();
  for (const entry of entries) {
    const inboundId = entry.kind === "request" ? entry.ids.request : entry.ids.start;
    if (requestEntries.has(inboundId) || subscriptionEntries.has(inboundId)) {
      throw new Error(`createDebugHost: duplicate entry for wire id ${inboundId}`);
    }
    if (entry.kind === "request") {
      requestEntries.set(entry.ids.request, entry);
    } else {
      subscriptionEntries.set(entry.ids.start, entry);
      stopToStart.set(entry.ids.stop, entry.ids.start);
    }
  }

  // Forwarded-subscription ledger for the dispose-time upstream stop:
  // start-frame requestIds seen on the forward path, cleared by their stop.
  const forwardedSubscriptions = new Map<string, number>(); // requestId -> stop frameId
  const stopIdByMethod = new Map<string, number>();
  for (const [frameId, info] of wireIndex) {
    if (info.kind === "stop") stopIdByMethod.set(info.method, frameId);
  }

  const mockSubscriptions = new Map<string, MockSubscriptionSlot>();
  let disposed = false;

  /**
   * Post through a pipe, disposing the debug host if the pipe throws, so
   * a dead provider can never break the router's message loop.
   **/
  function sendThrough(pipe: WireProvider, message: Uint8Array): void {
    try {
      pipe.postMessage(message);
    } catch {
      dispose();
    }
  }

  /**
   * Surface one frame to the observer and the decision listener, each
   * isolated so neither can break routing.
   **/
  function emit(
    direction: FrameDirection,
    requestId: string,
    frameId: number,
    byteLength: number,
    tier: DebugHostTier,
  ): void {
    if (!observe && !onDecision) return;
    const info = wireIndex.get(frameId);
    const frame: ObservedFrame = {
      direction,
      requestId,
      frameId,
      role: info?.kind ?? "unknown",
      byteLength,
      timestamp: Date.now(),
    };
    if (observe) {
      try {
        observe(frame);
      } catch {
        // an observer must never break routing
      }
    }
    if (onDecision) {
      try {
        onDecision({ tier, method: info?.method, frame });
      } catch {
        // a decision listener must never break routing
      }
    }
  }

  /** Encode one frame and send it to the product, marked `mock`. */
  function replyToProduct(requestId: string, frameId: number, payload: Uint8Array): void {
    if (disposed) return;
    const encoded = encodeWireMessage({ requestId, payload: { id: frameId, value: payload } });
    if (encoded.isErr()) return;
    emit("out", requestId, frameId, payload.length, "mock");
    sendThrough(provider, encoded.value);
  }

  function dispatchRequest(
    entry: DebugRequestEntry,
    requestId: string,
    payload: Uint8Array,
  ): void {
    Promise.resolve()
      .then(() => entry.handle({ requestId }, payload))
      .then(
        (response) => replyToProduct(requestId, entry.ids.response, response),
        () => {
          console.warn(
            `[truapi] debug host: mock handler for wire id=${entry.ids.request} threw on requestId=${requestId} — no response sent; the caller will hang`,
          );
        },
      );
  }

  function dispatchSubscriptionStart(
    entry: DebugSubscriptionEntry,
    requestId: string,
    payload: Uint8Array,
  ): void {
    if (mockSubscriptions.has(requestId)) return; // duplicate start: drop
    let closed = false;
    const port: DebugSubscriptionPort & { close(): void } = {
      sendReceive: (item) => {
        if (closed || disposed) return;
        replyToProduct(requestId, entry.ids.receive, item);
      },
      sendInterrupt: (reason) => {
        if (closed || disposed) return;
        closed = true;
        replyToProduct(requestId, entry.ids.interrupt, reason);
        mockSubscriptions.delete(requestId);
      },
      get isClosed() {
        return closed || disposed;
      },
      close: () => {
        closed = true;
      },
    };
    const slot: MockSubscriptionSlot = { port, stopped: false };
    mockSubscriptions.set(requestId, slot);
    Promise.resolve()
      .then(() => entry.start({ requestId }, payload, port))
      .then(
        (cleanup) => {
          if (slot.stopped || disposed) {
            try {
              cleanup();
            } catch {
              // cleanup errors are isolated
            }
            mockSubscriptions.delete(requestId);
            return;
          }
          slot.cleanup = cleanup;
        },
        () => {
          mockSubscriptions.delete(requestId);
          console.warn(
            `[truapi] debug host: mock subscription start for wire id=${entry.ids.start} threw on requestId=${requestId} — no interrupt sent; the caller will hang`,
          );
        },
      );
  }

  function stopMockSubscription(requestId: string): void {
    const slot = mockSubscriptions.get(requestId);
    if (!slot) return;
    slot.stopped = true;
    slot.port.close();
    mockSubscriptions.delete(requestId);
    if (slot.cleanup) {
      try {
        slot.cleanup();
      } catch {
        // handler cleanup errors are isolated from the router
      }
    }
  }

  // Router: product frames split by wire id — mock entries, forward pipe,
  // or loud unhandled.
  const unsubscribeProduct = provider.subscribe((message) => {
    if (disposed) return;
    const decoded = decodeWireMessage(message);
    if (decoded.isErr()) {
      // Undecodable envelope: with a real host attached, stay
      // byte-transparent and let it judge the frame; headless, there is
      // nothing to route on, so say so instead of hanging silently.
      if (forward) {
        sendThrough(forward, message);
      } else {
        console.warn(
          `[truapi] debug host: undecodable wire envelope (${message.length}B) — no forward pipe; a caller waiting on it will hang`,
        );
      }
      return;
    }
    const { requestId, payload } = decoded.value;

    const requestEntry = requestEntries.get(payload.id);
    if (requestEntry) {
      emit("in", requestId, payload.id, payload.value.length, "mock");
      dispatchRequest(requestEntry, requestId, payload.value);
      return;
    }
    const subscriptionEntry = subscriptionEntries.get(payload.id);
    if (subscriptionEntry) {
      emit("in", requestId, payload.id, payload.value.length, "mock");
      dispatchSubscriptionStart(subscriptionEntry, requestId, payload.value);
      return;
    }
    if (stopToStart.has(payload.id) && mockSubscriptions.has(requestId)) {
      emit("in", requestId, payload.id, payload.value.length, "mock");
      stopMockSubscription(requestId);
      return;
    }

    if (forward) {
      emit("in", requestId, payload.id, payload.value.length, "forward");
      // Ledger forwarded subscription lifecycles for the dispose-time stop.
      const info = wireIndex.get(payload.id);
      if (info?.kind === "start") {
        const stopId = stopIdByMethod.get(info.method);
        if (stopId !== undefined) forwardedSubscriptions.set(requestId, stopId);
      } else if (info?.kind === "stop") {
        forwardedSubscriptions.delete(requestId);
      }
      sendThrough(forward, message);
      return;
    }

    emit("in", requestId, payload.id, payload.value.length, "unhandled");
    if (!onDecision) {
      const method = wireIndex.get(payload.id)?.method ?? "unknown method";
      console.warn(
        `[truapi] debug host: unhandled frame ${method} (wire id=${payload.id}, requestId=${requestId}) — no mock entry and no forward pipe; the caller will hang`,
      );
    }
  });

  // Real-host answers relay back verbatim: same bytes, same requestId.
  const unsubscribeForward = forward?.subscribe((message) => {
    if (disposed) return;
    const decoded = decodeWireMessage(message);
    if (decoded.isOk()) {
      const { requestId, payload } = decoded.value;
      // A host-side interrupt ends the subscription; drop it from the ledger.
      if (wireIndex.get(payload.id)?.kind === "interrupt") {
        forwardedSubscriptions.delete(requestId);
      }
      if (observe || onDecision) {
        emit("out", requestId, payload.id, payload.value.length, "forward");
      }
    }
    sendThrough(provider, message);
  });

  const unsubscribeProductClose = provider.subscribeClose?.(() => dispose());
  const unsubscribeForwardClose = forward?.subscribeClose?.(() => dispose());

  function dispose(): void {
    if (disposed) return;
    disposed = true;
    // Stop every live forwarded subscription upstream so the real host
    // does not keep streaming into a detached pipe.
    if (forward) {
      for (const [requestId, stopId] of forwardedSubscriptions) {
        const encoded = encodeWireMessage({
          requestId,
          payload: { id: stopId, value: new Uint8Array(0) },
        });
        if (encoded.isOk()) {
          try {
            forward.postMessage(encoded.value);
          } catch {
            break; // pipe already dead; nothing more to stop
          }
        }
      }
      forwardedSubscriptions.clear();
    }
    for (const requestId of [...mockSubscriptions.keys()]) {
      stopMockSubscription(requestId);
    }
    for (const unsubscribe of [
      unsubscribeProduct,
      unsubscribeForward,
      unsubscribeProductClose,
      unsubscribeForwardClose,
    ]) {
      try {
        unsubscribe?.();
      } catch {
        // detach errors are ignored during teardown
      }
    }
  }

  return { dispose };
}
