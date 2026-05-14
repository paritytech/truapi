import {
  decodeWireMessage,
  encodeWireMessage,
  type Provider,
  type RequestFrameIds,
  type SubscriptionFrameIds,
} from "@parity/truapi";

/**
 * Per-call context handed to every host handler. Carries the wire
 * `requestId` so handlers can correlate audit logs or look up state
 * scoped to a single inbound call.
 **/
export interface CallContext {
  /**
   * Transport-assigned request id for the originating client frame.
   **/
  readonly requestId: string;
}

/**
 * Sink handed to subscription handlers. Encoders for the item and
 * (optional) interrupt payload are baked in by the generator; handlers
 * supply already-typed values.
 **/
export interface SubscriptionSink<Item, Reason = never> {
  /**
   * Emit one subscription item to the client.
   **/
  send(item: Item): void;

  /**
   * Interrupt the subscription with a typed reason. Only available for
   * methods declared as `ResultSubscription` (typed interrupt payload).
   **/
  interrupt(reason: Reason): void;

  /**
   * `true` once the subscription has been torn down (either by a `stop`
   * frame from the client, an `interrupt` from the host, or transport
   * close). Handlers may inspect this to short-circuit work.
   **/
  readonly isClosed: boolean;
}

/**
 * Cleanup function returned by a subscription handler. Invoked when the
 * client stops the subscription, when the handler interrupts it, or when
 * the underlying provider closes.
 **/
export type SubscriptionCleanup = () => void;

/**
 * Handler entry for a one-shot request method. The dispatcher decodes
 * inbound wire bytes, invokes `handle`, and forwards the returned bytes
 * as the response frame body.
 **/
export interface RequestEntry {
  readonly kind: "request";
  readonly ids: RequestFrameIds;

  /**
   * Decode the inbound request bytes, run the handler, and produce the
   * SCALE-encoded response payload bytes.
   **/
  handle(payload: Uint8Array, ctx: CallContext): Promise<Uint8Array>;
}

/**
 * Handler entry for a subscription method.
 **/
export interface SubscriptionEntry {
  readonly kind: "subscription";
  readonly ids: SubscriptionFrameIds;

  /**
   * Decode the inbound start payload, build a `SubscriptionSink` bound
   * to this requestId, run the handler, and return a cleanup function.
   **/
  start(
    payload: Uint8Array,
    ctx: CallContext,
    framePort: SubscriptionFramePort,
  ): Promise<SubscriptionCleanup> | SubscriptionCleanup;
}

/**
 * Raw byte port a generated subscription entry uses to push receive and
 * interrupt frames back to the client.
 **/
export interface SubscriptionFramePort {
  /**
   * Emit a receive frame carrying the supplied encoded item bytes.
   **/
  sendReceive(payload: Uint8Array): void;

  /**
   * Emit an interrupt frame carrying the supplied encoded reason bytes
   * and close the subscription locally.
   **/
  sendInterrupt(payload: Uint8Array): void;

  /**
   * `true` once the subscription has ended.
   **/
  readonly isClosed: boolean;
}

/**
 * Composed dispatch table for a host server. Generated `server.ts`
 * builds this from the wire-table and supplied typed handlers.
 **/
export type HostDispatchEntry = RequestEntry | SubscriptionEntry;

/**
 * Optional hooks for visibility into protocol drift or handler errors.
 **/
export interface HostServerHooks {
  /**
   * Called when an inbound frame's wire id is not present in the
   * dispatch table. Default: drop silently.
   **/
  onUnknownFrame?(payload: { id: number; value: Uint8Array }): void;

  /**
   * Called when a request handler throws or rejects. The dispatcher
   * does not send a response frame, the client request will hang or
   * time out per its own policy. Default: swallow.
   **/
  onRequestHandlerError?(
    ids: RequestFrameIds,
    error: Error,
    ctx: CallContext,
  ): void;

  /**
   * Called when a subscription handler throws or rejects during
   * `start`. Default: swallow.
   **/
  onSubscriptionStartError?(
    ids: SubscriptionFrameIds,
    error: Error,
    ctx: CallContext,
  ): void;
}

/**
 * Handle returned by `createHostServer`.
 **/
export interface TrUApiHostServer {
  /**
   * Detach all provider listeners, drop pending subscription state, and
   * release resources. Does not dispose the underlying `Provider`; the
   * caller decides whether to also dispose it.
   **/
  dispose(): void;
}

/**
 * Convert an arbitrary thrown value into an `Error` instance.
 **/
function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

interface DispatchTable {
  byRequest: Map<number, RequestEntry>;
  byStart: Map<number, SubscriptionEntry>;
  stopIds: Set<number>;
  startToStop: Map<number, number>;
}

/**
 * Index a list of dispatch entries by wire id for O(1) lookup.
 **/
function buildDispatchTable(entries: HostDispatchEntry[]): DispatchTable {
  const byRequest = new Map<number, RequestEntry>();
  const byStart = new Map<number, SubscriptionEntry>();
  const stopIds = new Set<number>();
  const startToStop = new Map<number, number>();

  for (const entry of entries) {
    if (entry.kind === "request") {
      const existing = byRequest.get(entry.ids.request);
      if (existing) {
        throw new Error(
          `duplicate request wire id ${entry.ids.request}; both entries claim it`,
        );
      }
      byRequest.set(entry.ids.request, entry);
    } else {
      const existing = byStart.get(entry.ids.start);
      if (existing) {
        throw new Error(
          `duplicate subscription start wire id ${entry.ids.start}; both entries claim it`,
        );
      }
      byStart.set(entry.ids.start, entry);
      stopIds.add(entry.ids.stop);
      startToStop.set(entry.ids.start, entry.ids.stop);
    }
  }

  return { byRequest, byStart, stopIds, startToStop };
}

interface ActiveSubscription {
  readonly entry: SubscriptionEntry;
  readonly cleanup: SubscriptionCleanup;
  readonly port: SubscriptionFramePort;
}

/**
 * Wire a host server to a `Provider`. The server subscribes to inbound
 * frames, routes by wire id, and emits responses, receive items, and
 * interrupts back through the same provider. Idempotent disposal.
 **/
export function createHostServer(
  provider: Provider,
  entries: HostDispatchEntry[],
  hooks: HostServerHooks = {},
): TrUApiHostServer {
  const table = buildDispatchTable(entries);
  const activeSubscriptions = new Map<string, ActiveSubscription>();
  let disposed = false;

  /**
   * Send a single wire frame through the provider. Swallows errors that
   * happen after disposal; surfaces other send errors as host-server
   * disposal.
   **/
  function send(requestId: string, id: number, value: Uint8Array): void {
    if (disposed) return;
    const encoded = encodeWireMessage({
      requestId,
      payload: { id, value },
    });
    if (encoded.isErr()) {
      throw encoded.error;
    }
    try {
      provider.postMessage(encoded.value);
    } catch (error) {
      if (disposed) return;
      throw toError(error);
    }
  }

  /**
   * Look up the dispatch entry for a subscription frame's stop id and
   * remove the active subscription, invoking its cleanup.
   **/
  function tearDownSubscription(requestId: string): void {
    const active = activeSubscriptions.get(requestId);
    if (!active) return;
    activeSubscriptions.delete(requestId);
    try {
      active.cleanup();
    } catch {
      // handler cleanup errors are isolated from the dispatcher
    }
  }

  /**
   * Build a frame port for a subscription identified by its start
   * frame's requestId. The port emits receive/interrupt frames and
   * tracks the closed flag.
   **/
  function makeFramePort(
    requestId: string,
    ids: SubscriptionFrameIds,
  ): SubscriptionFramePort {
    let closed = false;
    return {
      sendReceive(payload) {
        if (closed || disposed) return;
        send(requestId, ids.receive, payload);
      },
      sendInterrupt(payload) {
        if (closed || disposed) return;
        closed = true;
        send(requestId, ids.interrupt, payload);
        // The host has interrupted the subscription locally. Remove it
        // from the active map so further stop frames are no-ops, but do
        // not re-invoke the handler-supplied cleanup, the handler is in
        // charge of its own teardown when it called `interrupt`.
        activeSubscriptions.delete(requestId);
      },
      get isClosed() {
        return closed || disposed;
      },
    };
  }

  /**
   * Decode one inbound wire frame and dispatch it.
   **/
  function handleInbound(message: Uint8Array): void {
    if (disposed) return;
    const decoded = decodeWireMessage(message);
    if (decoded.isErr()) {
      return;
    }
    const { requestId, payload } = decoded.value;
    const ctx: CallContext = { requestId };

    const requestEntry = table.byRequest.get(payload.id);
    if (requestEntry) {
      Promise.resolve()
        .then(() => requestEntry.handle(payload.value, ctx))
        .then(
          (responseBytes) => {
            send(requestId, requestEntry.ids.response, responseBytes);
          },
          (error) => {
            hooks.onRequestHandlerError?.(
              requestEntry.ids,
              toError(error),
              ctx,
            );
          },
        );
      return;
    }

    const subEntry = table.byStart.get(payload.id);
    if (subEntry) {
      if (activeSubscriptions.has(requestId)) {
        // A second start frame for the same requestId is a protocol
        // violation, drop it.
        return;
      }
      const port = makeFramePort(requestId, subEntry.ids);
      Promise.resolve()
        .then(() => subEntry.start(payload.value, ctx, port))
        .then(
          (cleanup) => {
            if (disposed || port.isClosed) {
              try {
                cleanup();
              } catch {
                // ignore
              }
              return;
            }
            activeSubscriptions.set(requestId, {
              entry: subEntry,
              cleanup,
              port,
            });
          },
          (error) => {
            hooks.onSubscriptionStartError?.(
              subEntry.ids,
              toError(error),
              ctx,
            );
          },
        );
      return;
    }

    if (table.stopIds.has(payload.id)) {
      tearDownSubscription(requestId);
      return;
    }

    hooks.onUnknownFrame?.({ id: payload.id, value: payload.value });
  }

  const unsubscribeMessage = provider.subscribe(handleInbound);
  const unsubscribeClose = provider.subscribeClose?.(() => {
    dispose();
  });

  /**
   * Tear down every active subscription and detach provider listeners.
   * Idempotent.
   **/
  function dispose(): void {
    if (disposed) return;
    disposed = true;
    for (const [requestId, active] of activeSubscriptions) {
      activeSubscriptions.delete(requestId);
      try {
        active.cleanup();
      } catch {
        // ignore handler cleanup errors during shutdown
      }
    }
    try {
      unsubscribeMessage();
    } catch {
      // ignore
    }
    try {
      unsubscribeClose?.();
    } catch {
      // ignore
    }
  }

  return {
    dispose,
  };
}
