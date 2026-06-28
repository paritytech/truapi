import { concatBytes } from "@noble/hashes/utils.js";
import { err, ok, type Result, type ResultAsync } from "neverthrow";

import { str, u8, type ResultPayload } from "./scale.js";

/**
 * Coerce an unknown thrown value into an `Error` instance.
 */
function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

/**
 * Handle returned by TrUAPI subscription APIs.
 **/
export interface Subscription {
  /**
   * Stop the subscription. Calling this more than once has no additional effect.
   **/
  unsubscribe: () => void;

  /**
   * Transport-assigned request id for the subscription start frame.
   *
   * Methods that accept a `followSubscriptionId` use this value to scope
   * follow-up requests to a specific active subscription.
   **/
  subscriptionId: string;
}

/**
 * Terminal error delivered through `Observer.error` for every non-normal
 * subscription end. When the peer interrupted the stream with a typed payload,
 * `reason` carries the decoded `Reason`; otherwise `reason` is `undefined` and
 * the underlying transport/decode error is preserved on `cause`.
 *
 * Discriminate with `error.reason !== undefined` (or `'reason' in error`).
 **/
export class SubscriptionError<Reason = never> extends Error {
  /**
   * Typed payload supplied by the peer when it interrupted the subscription.
   * `undefined` when the stream ended for any other reason (transport close,
   * decode failure, malformed interrupt payload).
   **/
  readonly reason?: Reason;

  constructor(message: string, options?: { reason?: Reason; cause?: unknown }) {
    super(
      message,
      options?.cause !== undefined ? { cause: options.cause } : undefined,
    );
    this.name = "SubscriptionError";
    if (options?.reason !== undefined) this.reason = options.reason;
  }
}

/**
 * Minimal Observable-compatible observer shape used by generated subscription
 * APIs without depending on RxJS.
 *
 * `Reason` is the typed interrupt payload for the originating subscription.
 * Methods without a typed interrupt resolve `Reason` to `never`, leaving
 * `error.reason` typed as `undefined`.
 **/
export interface Observer<Item, Reason = never> {
  /**
   * Called with each successfully decoded subscription item.
   **/
  next(value: Item): void;

  /**
   * Called once when the stream terminates with an error. Inspect
   * `error.reason` to distinguish a typed peer interrupt from a transport or
   * decode failure (`error.cause` carries the underlying failure in the
   * latter case).
   **/
  error(error: SubscriptionError<Reason>): void;

  /**
   * Called once when the peer normally completes the stream.
   **/
  complete(): void;
}

declare global {
  interface SymbolConstructor {
    readonly observable: unique symbol;
  }
}

/**
 * Minimal Observable-compatible object returned by generated subscription APIs.
 *
 * Implements the ES Observable interop protocol so that consumers can pass
 * an instance straight to `rxjs.from(...)`.
 **/
export interface ObservableLike<Item, Reason = never> {
  /**
   * Start the stream and receive `next`, `error`, and `complete` callbacks.
   **/
  subscribe(observer?: Partial<Observer<Item, Reason>>): Subscription;
  /**
   * Observable interop hook. Returns `this`.
   **/
  [Symbol.observable](): ObservableLike<Item, Reason>;
}

/**
 * Numeric frame ids for a one-shot request method.
 **/
export interface RequestFrameIds {
  /**
   * Wire discriminant for the outbound request frame.
   **/
  request: number;

  /**
   * Wire discriminant for the inbound response frame.
   **/
  response: number;
}

/**
 * Numeric frame ids for a subscription method.
 **/
export interface SubscriptionFrameIds {
  /**
   * Wire discriminant for the outbound start frame.
   **/
  start: number;

  /**
   * Wire discriminant for the outbound stop frame.
   **/
  stop: number;

  /**
   * Wire discriminant for the inbound interrupt frame.
   **/
  interrupt: number;

  /**
   * Wire discriminant for the inbound receive frame.
   **/
  receive: number;
}

/**
 * Options accepted by `TrUApiTransport.request`.
 **/
export interface RequestParams<Ok, Err> {
  /**
   * Wire discriminants for this request method.
   **/
  ids: RequestFrameIds;

  /**
   * SCALE-encoded request payload bytes.
   **/
  payload: Uint8Array;

  /**
   * Decode SCALE response payload bytes into the wire `ResultPayload`
   * envelope. The transport unwraps the envelope into `ResultAsync<Ok, Err>`.
   **/
  decodeResponse: (payload: Uint8Array) => ResultPayload<Ok, Err>;
}

/**
 * Options accepted by `TrUApiTransport.subscribeRaw`.
 **/
export interface SubscribeRawParams {
  /**
   * Wire discriminants for this subscription method.
   **/
  ids: SubscriptionFrameIds;

  /**
   * SCALE-encoded subscription start payload bytes.
   **/
  payload: Uint8Array;

  /**
   * Called with raw SCALE receive payload bytes.
   **/
  onReceive: (payload: Uint8Array) => void;

  /**
   * Called with raw SCALE interrupt payload bytes when the peer interrupts the subscription.
   **/
  onInterrupt?: (payload: Uint8Array) => void;

  /**
   * Called when the underlying provider closes while the subscription is active.
   **/
  onClose?: (error: Error) => void;
}

/**
 * Byte-level transport used by generated client stubs.
 **/
export interface TrUApiTransport {
  /**
   * SCALE codec version used by generated handshake calls.
   *
   * @deprecated TODO(shared-core-wire): remove this public transport field once
   * generated handshake requests read `TRUAPI_CODEC_VERSION` directly instead
   * of going through transport state.
   **/
  readonly codecVersion: number;

  /**
   * Send a one-shot request and resolve with the typed Ok/Err outcome.
   **/
  request<Ok, Err>(params: RequestParams<Ok, Err>): ResultAsync<Ok, Err>;

  /**
   * Start a subscription and return a handle that can stop it.
   **/
  subscribeRaw(params: SubscribeRawParams): Subscription;

  /**
   * Tear down the transport and release the listeners it registered on the
   * underlying `WireProvider`. Pending requests reject and live subscriptions
   * receive `onClose`. Idempotent.
   *
   * The provider itself is left alone; the caller decides whether to also
   * call `provider.dispose()` (long-lived hosts that swap providers will
   * typically dispose the transport but keep the provider).
   **/
  dispose(): void;
}

/**
 * Tagged payload inside a TrUAPI wire frame.
 **/
export interface Payload {
  /**
   * Wire-table numeric discriminant.
   **/
  id: number;

  /**
   * SCALE-encoded payload body.
   **/
  value: Uint8Array;
}

/**
 * Top-level TrUAPI wire message.
 **/
export interface ProtocolMessage {
  /**
   * Request id used to correlate request/response and subscription frames.
   **/
  requestId: string;

  /**
   * Tagged SCALE payload carried by this frame.
   **/
  payload: Payload;
}

/**
 * Raw SCALE-wire-frame pipe abstraction used by the transport. A `WireProvider`
 * is the low-level channel (a `MessagePort` or iframe `postMessage` link) that
 * carries encoded frames between the product and the host.
 **/
export interface WireProvider {
  /**
   * Send a complete SCALE-encoded wire frame to the peer.
   **/
  postMessage(message: Uint8Array): void;

  /**
   * Register a callback for inbound SCALE-encoded wire frames.
   **/
  subscribe(callback: (message: Uint8Array) => void): () => void;

  /**
   * Register a callback for provider-level close or failure events.
   *
   * Providers keep a terminal close reason. The callback fires at most once
   * for an active subscription, and fires immediately when registered after
   * the provider has already closed.
   **/
  subscribeClose?(callback: (error: Error) => void): () => void;

  /**
   * Release provider resources and close the underlying pipe.
   **/
  dispose(): void;
}

/**
 * Encode a `ProtocolMessage` into a SCALE wire frame.
 **/
export function encodeWireMessage(
  message: ProtocolMessage,
): Result<Uint8Array, Error> {
  const id = message.payload.id;
  if (!Number.isInteger(id) || id < 0 || id > 255) {
    return err(new Error(`Invalid wire discriminant: ${id}`));
  }
  return ok(
    concatBytes(str.enc(message.requestId), u8.enc(id), message.payload.value),
  );
}

/**
 * Decode a SCALE wire frame into a `ProtocolMessage`.
 **/
export function decodeWireMessage(
  message: Uint8Array,
): Result<ProtocolMessage, Error> {
  if (message.length < 1) {
    return err(new Error("Wire frame too short: empty buffer"));
  }
  let cursor = message;
  const requestIdEndResult = scanStrEnd(cursor);
  if (requestIdEndResult.isErr()) {
    return err(requestIdEndResult.error);
  }
  const requestIdEnd = requestIdEndResult.value;
  const requestId = str.dec(cursor.subarray(0, requestIdEnd));
  cursor = cursor.subarray(requestIdEnd);
  if (cursor.length < 1) {
    return err(new Error("Wire frame too short: missing discriminant byte"));
  }
  const id = cursor[0];
  const value = cursor.subarray(1);
  // Hand the value bytes back as a fresh slice so callers may safely retain
  // it even if the source buffer is reused by the transport.
  const valueCopy = new Uint8Array(value.length);
  valueCopy.set(value);
  return ok({ requestId, payload: { id, value: valueCopy } });
}

/**
 * Return the byte offset just past the leading SCALE-encoded string.
 **/
function scanStrEnd(bytes: Uint8Array): Result<number, Error> {
  if (bytes.length < 1) {
    return err(new Error("compact-len: empty buffer"));
  }
  const first = bytes[0];
  const mode = first & 0b11;
  let lengthLen: number;
  let strLen: number;
  if (mode === 0) {
    lengthLen = 1;
    strLen = first >> 2;
  } else if (mode === 1) {
    if (bytes.length < 2) {
      return err(new Error("compact-len: truncated mode-1 prefix"));
    }
    lengthLen = 2;
    strLen = ((first >> 2) | (bytes[1] << 6)) & 0x3fff;
  } else if (mode === 2) {
    if (bytes.length < 4) {
      return err(new Error("compact-len: truncated mode-2 prefix"));
    }
    lengthLen = 4;
    strLen =
      ((first >> 2) | (bytes[1] << 6) | (bytes[2] << 14) | (bytes[3] << 22)) >>>
      0;
  } else {
    // big-int mode: not used for requestId in our protocol
    return err(
      new Error("compact big-int mode not supported in wire envelope"),
    );
  }
  const total = lengthLen + strLen;
  if (total > bytes.length) {
    return err(new Error("compact-len: declared length exceeds buffer"));
  }
  return ok(total);
}

/**
 * Internal listener bookkeeping and close-once state machine shared by the
 * built-in `WireProvider` implementations. Transport-specific code wires its
 * inbound source to `deliver`, registers cleanup via `onClose`, and exposes
 * `subscribe`/`subscribeClose` to callers.
 **/
function createBaseProvider() {
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  const onCloseCleanup = new Set<() => void>();
  let closedError: Error | null = null;

  return {
    /** Current close error, or `null` while the provider is open. */
    closed: (): Error | null => closedError,

    /** Dispatch an inbound frame to every active subscriber. */
    deliver(message: Uint8Array) {
      if (closedError) return;
      for (const listener of [...listeners]) listener(message);
    },

    /** Transition to the closed state. Idempotent. */
    close(error: unknown) {
      if (closedError) return;
      closedError = toError(error);
      for (const fn of [...onCloseCleanup]) {
        try {
          fn();
        } catch {
          // ignore cleanup failure
        }
      }
      onCloseCleanup.clear();
      for (const listener of [...closeListeners]) listener(closedError);
      listeners.clear();
      closeListeners.clear();
    },

    /** Register a cleanup function to run exactly once when `close` fires. */
    onClose(fn: () => void) {
      if (closedError) {
        try {
          fn();
        } catch {
          // ignore cleanup failure
        }
        return;
      }
      onCloseCleanup.add(fn);
    },

    /** Register an inbound message listener. No-op after close. */
    subscribe(callback: (message: Uint8Array) => void): () => void {
      if (closedError) return () => {};
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },

    /**
     * Register a close listener. If the provider is already closed, the
     * callback fires immediately with the stored error.
     **/
    subscribeClose(callback: (error: Error) => void): () => void {
      if (closedError) {
        callback(closedError);
        return () => {};
      }
      closeListeners.add(callback);
      return () => {
        closeListeners.delete(callback);
      };
    },
  };
}

/**
 * Create a provider for the child side of an iframe `postMessage` channel.
 *
 * `target` is the `Window` the provider posts to (typically `window.parent`);
 * `hostOrigin` is the pinned `targetOrigin` for outbound frames and the
 * required `event.origin` of inbound frames. The provider only delivers
 * frames whose `event.source === target` and `event.origin === hostOrigin`,
 * so it cannot be coerced by an unrelated frame parent.
 **/
export function createIframeProvider(options: {
  target: Window;
  hostOrigin: string;
}): WireProvider {
  const base = createBaseProvider();
  const { target, hostOrigin } = options;

  const onMessage = (event: MessageEvent) => {
    if (event.source !== target) return;
    if (event.origin !== hostOrigin) return;
    if (!(event.data instanceof Uint8Array)) return;
    base.deliver(event.data);
  };
  window.addEventListener("message", onMessage);
  base.onClose(() => window.removeEventListener("message", onMessage));

  return {
    postMessage(message) {
      const error = base.closed();
      if (error) throw error;
      try {
        target.postMessage(message, hostOrigin);
      } catch (error) {
        base.close(error);
        throw toError(error);
      }
    },
    subscribe: base.subscribe,
    subscribeClose: base.subscribeClose,
    dispose() {
      base.close(new Error("iframe provider disposed"));
    },
  };
}

/**
 * Create a provider from a web or Electron `MessagePort`.
 **/
export function createMessagePortProvider(
  port: MessagePort | Promise<MessagePort>,
): WireProvider {
  const base = createBaseProvider();
  let resolvedPort: MessagePort | null = null;
  const pending: Uint8Array[] = [];

  void Promise.resolve(port)
    .then((p) => {
      if (base.closed()) {
        try {
          p.close();
        } catch {
          // ignore duplicate close during shutdown
        }
        return;
      }

      resolvedPort = p;
      p.onmessage = (event: MessageEvent) => {
        if (event.data instanceof Uint8Array) base.deliver(event.data);
      };
      if ("onmessageerror" in p) {
        p.onmessageerror = () => {
          base.close(new Error("message port closed unexpectedly"));
        };
      }
      p.start();
      for (const msg of pending) p.postMessage(msg);
      pending.length = 0;
      base.onClose(() => {
        try {
          p.close();
        } catch {
          // ignore duplicate close during shutdown
        }
      });
    })
    .catch((error: unknown) => {
      base.close(error);
    });

  return {
    postMessage(message) {
      const error = base.closed();
      if (error) throw error;
      if (resolvedPort) {
        try {
          resolvedPort.postMessage(message);
        } catch (error) {
          base.close(error);
          throw toError(error);
        }
      } else {
        pending.push(message);
      }
    },
    subscribe: base.subscribe,
    subscribeClose: base.subscribeClose,
    dispose() {
      base.close(new Error("message port provider disposed"));
      pending.length = 0;
    },
  };
}
