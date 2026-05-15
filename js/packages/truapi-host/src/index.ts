import type { Result } from "neverthrow";

import {
  decodeWireMessage,
  encodeWireMessage,
  type Provider,
  type RequestFrameIds,
  type SubscriptionFrameIds,
} from "@parity/truapi";

/**
 * Map a handler's versioned `Result<{tag, value: Ok}, {tag, value: Err}>`
 * into the wire-shape `{ tag, value: { success, value } }` the response
 * codec encodes. The version tag flows from whichever arm is settled, so a
 * V1 handler return stays V1 on the wire.
 *
 * Generated dispatcher entries call this so the per-method `await
 * handler.match(...)` boilerplate lives in one place instead of being
 * cloned at every request method.
 **/
/**
 * `Versioned<V, T>` mirrors how generated unions render unit variants: a
 * unit `value` becomes `value?: undefined` so handlers can return
 * `{ tag: "V1" }` without naming the field. Non-unit values keep `value: T`
 * as required.
 **/
type Versioned<V extends string, T> = [T] extends [undefined]
  ? { tag: V; value?: undefined }
  : { tag: V; value: T };

export function toResponsePayload<V extends string, Ok, Err>(
  result: Result<Versioned<V, Ok>, Versioned<V, Err>>,
): {
  tag: V;
  value: { success: true; value: Ok } | { success: false; value: Err };
} {
  return result.match(
    (ok) => ({
      tag: ok.tag,
      // `value` is optional in unit-variant inputs; reading it returns
      // `undefined` which is what the wire codec expects in that arm.
      value: { success: true as const, value: ok.value as Ok },
    }),
    (err) => ({
      tag: err.tag,
      value: { success: false as const, value: err.value as Err },
    }),
  );
}

/**
 * Flat counterpart to [`toResponsePayload`] for methods that carry no
 * version wrapper on either side. Generated for the unversioned dispatch
 * path; collapses a `Result<Ok, Err>` into the wire `{ success, value }`
 * envelope without a tag.
 **/
export function toFlatResponsePayload<Ok, Err>(
  result: Result<Ok, Err>,
): { success: true; value: Ok } | { success: false; value: Err } {
  return result.match(
    (value) => ({ success: true as const, value }),
    (value) => ({ success: false as const, value }),
  );
}

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
  handle(ctx: CallContext, payload: Uint8Array): Promise<Uint8Array>;
}

/**
 * Handler entry for a subscription method. The generator subscribes to
 * the handler's `ObservableLike` inside `start` and bridges the resulting
 * `Observer` callbacks to wire frames through `SubscriptionFramePort`.
 **/
export interface SubscriptionEntry {
  readonly kind: "subscription";
  readonly ids: SubscriptionFrameIds;

  /**
   * Decode the inbound start payload, subscribe to the handler's
   * observable, and return a cleanup function.
   **/
  start(
    ctx: CallContext,
    payload: Uint8Array,
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
}

/**
 * Index a list of dispatch entries by wire id for O(1) lookup.
 **/
function buildDispatchTable(entries: HostDispatchEntry[]): DispatchTable {
  const byRequest = new Map<number, RequestEntry>();
  const byStart = new Map<number, SubscriptionEntry>();
  const stopIds = new Set<number>();

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
    }
  }

  return { byRequest, byStart, stopIds };
}

/**
 * In-progress slot from when a `start` frame arrives until the handler's
 * `start` function returns the cleanup. A `stop` frame that lands during
 * this window flips `stopped`, so the start-resolution path can tear the
 * subscription back down instead of registering an orphaned active slot.
 **/
interface PendingSubscription {
  readonly kind: "pending";
  readonly port: SubscriptionFramePort;
  stopped: boolean;
}

/**
 * Fully-registered subscription: `start` returned, `cleanup` is captured,
 * the slot is the authority for stop/interrupt teardown.
 **/
interface ActiveSubscription {
  readonly kind: "active";
  readonly cleanup: SubscriptionCleanup;
  readonly port: SubscriptionFramePort;
}

type SubscriptionSlot = PendingSubscription | ActiveSubscription;

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
  const subscriptions = new Map<string, SubscriptionSlot>();
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
   * Tear down a subscription identified by its originating request id.
   * If the slot is still pending (handler's `start` has not returned yet),
   * record the stop so the start-resolution path can clean up without
   * leaving the handler subscribed.
   **/
  function tearDownSubscription(requestId: string): void {
    const slot = subscriptions.get(requestId);
    if (!slot) return;
    if (slot.kind === "pending") {
      slot.stopped = true;
      return;
    }
    subscriptions.delete(requestId);
    try {
      slot.cleanup();
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
        // The host has interrupted the subscription locally. Drop the
        // slot so further stop frames are no-ops; the handler is in
        // charge of its own teardown when it called `interrupt`.
        subscriptions.delete(requestId);
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
      let pending: Promise<Uint8Array>;
      try {
        pending = requestEntry.handle(ctx, payload.value);
      } catch (error) {
        hooks.onRequestHandlerError?.(requestEntry.ids, toError(error), ctx);
        return;
      }
      pending.then(
        (responseBytes) => {
          send(requestId, requestEntry.ids.response, responseBytes);
        },
        (error) => {
          hooks.onRequestHandlerError?.(requestEntry.ids, toError(error), ctx);
        },
      );
      return;
    }

    const subEntry = table.byStart.get(payload.id);
    if (subEntry) {
      if (subscriptions.has(requestId)) {
        // A second start frame for the same requestId is a protocol
        // violation, drop it.
        return;
      }
      const port = makeFramePort(requestId, subEntry.ids);
      const pending: PendingSubscription = {
        kind: "pending",
        port,
        stopped: false,
      };
      // Reserve the slot synchronously so that a stop frame arriving
      // before `start` resolves can mark it stopped instead of finding
      // an empty map.
      subscriptions.set(requestId, pending);

      const finish = (cleanup: SubscriptionCleanup): void => {
        const current = subscriptions.get(requestId);
        if (
          current !== pending ||
          disposed ||
          pending.stopped ||
          port.isClosed
        ) {
          if (current === pending) subscriptions.delete(requestId);
          try {
            cleanup();
          } catch {
            // ignore
          }
          return;
        }
        subscriptions.set(requestId, {
          kind: "active",
          cleanup,
          port,
        });
      };
      const fail = (error: unknown): void => {
        if (subscriptions.get(requestId) === pending) {
          subscriptions.delete(requestId);
        }
        hooks.onSubscriptionStartError?.(subEntry.ids, toError(error), ctx);
      };

      let startResult: Promise<SubscriptionCleanup> | SubscriptionCleanup;
      try {
        startResult = subEntry.start(ctx, payload.value, port);
      } catch (error) {
        fail(error);
        return;
      }
      if (startResult instanceof Promise) {
        startResult.then(finish, fail);
      } else {
        finish(startResult);
      }
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
    for (const [requestId, slot] of subscriptions) {
      subscriptions.delete(requestId);
      if (slot.kind === "pending") {
        // The pending slot's start-resolution path will see `disposed`
        // and invoke its own cleanup.
        slot.stopped = true;
        continue;
      }
      try {
        slot.cleanup();
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

/**
 * Generated typed handler interfaces, per-method aliases, and the
 * `createTrUApiServer(provider, handlers)` factory bound to this core.
 **/
export * from "./generated/server.js";
