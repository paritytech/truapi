import { err, ok, type Result } from "neverthrow";

import { str, u8 } from "./scale.js";

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
 * Minimal Observable-compatible observer shape used by generated subscription
 * APIs without depending on RxJS.
 **/
export interface Observer<Item> {
  /**
   * Called with each successfully decoded subscription item.
   **/
  next(value: Item): void;

  /**
   * Called once when the stream terminates with an error.
   **/
  error(error: Error): void;

  /**
   * Called once when the peer normally completes the stream.
   **/
  complete(): void;
}

/**
 * Minimal Observable-compatible object returned by generated subscription APIs.
 **/
export interface ObservableLike<Item> {
  /**
   * Start the stream and receive `next`, `error`, and `complete` callbacks.
   **/
  subscribe(observer?: Partial<Observer<Item>>): Subscription;
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
export interface RequestParams<Response> {
  /**
   * Wire discriminants for this request method.
   **/
  ids: RequestFrameIds;

  /**
   * SCALE-encoded request payload bytes.
   **/
  payload: Uint8Array;

  /**
   * Decode SCALE response payload bytes into the generated client return type.
   **/
  decodeResponse: (payload: Uint8Array) => Response;
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
   * Highest TrUAPI protocol version supported by this generated client.
   **/
  readonly truapiVersion: number;

  /**
   * SCALE codec version negotiated through the handshake.
   **/
  readonly codecVersion: number;

  /**
   * Send a one-shot request and resolve with the decoded response payload.
   **/
  request<Response>(params: RequestParams<Response>): Promise<Response>;

  /**
   * Start a subscription and return a handle that can stop it.
   **/
  subscribeRaw(params: SubscribeRawParams): Subscription;

  /**
   * Tear down the transport and release the listeners it registered on the
   * underlying `Provider`. Pending requests reject and live subscriptions
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
 * Raw message pipe abstraction used by the transport.
 **/
export interface Provider {
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
   **/
  subscribeClose?(callback: (error: Error) => void): () => void;

  /**
   * Release provider resources and close the underlying pipe.
   **/
  dispose(): void;
}

/**
 * Concatenate byte arrays without mutating the source arrays.
 **/
function concatBytes(parts: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const p of parts) total += p.length;
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
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
    concatBytes([
      str.enc(message.requestId),
      u8.enc(id),
      message.payload.value,
    ]),
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
 * Create a provider from a web or Electron `MessagePort`.
 **/
export function createMessagePortProvider(
  port: MessagePort | Promise<MessagePort>,
): Provider {
  let resolvedPort: MessagePort | null = null;
  let closedError: Error | null = null;
  const pending: Uint8Array[] = [];
  const listeners: Array<(message: Uint8Array) => void> = [];
  const closeListeners: Array<(error: Error) => void> = [];

  /**
   * Notify close listeners once and drop queued outbound messages.
   **/
  function notifyClose(error: unknown) {
    const nextError = error instanceof Error ? error : new Error(String(error));
    if (closedError) {
      return;
    }

    closedError = nextError;
    pending.length = 0;
    for (const listener of [...closeListeners]) {
      listener(nextError);
    }
  }

  void Promise.resolve(port)
    .then((p) => {
      if (closedError) {
        try {
          p.close();
        } catch {
          // ignore duplicate close during shutdown
        }
        return;
      }

      resolvedPort = p;
      p.onmessage = (event: MessageEvent) => {
        const data = event.data;
        if (!(data instanceof Uint8Array)) return;
        for (const listener of [...listeners]) listener(data);
      };
      if ("onmessageerror" in p) {
        p.onmessageerror = () => {
          notifyClose(new Error("message port closed unexpectedly"));
        };
      }
      p.start();
      for (const msg of pending) p.postMessage(msg);
      pending.length = 0;
    })
    .catch((error: unknown) => {
      notifyClose(error);
    });

  return {
    /**
     * Send bytes through the resolved port or queue them until it resolves.
     **/
    postMessage(message) {
      if (closedError) {
        throw closedError;
      }

      if (resolvedPort) {
        try {
          resolvedPort.postMessage(message);
        } catch (error) {
          notifyClose(error);
          throw error instanceof Error ? error : new Error(String(error));
        }
      } else {
        pending.push(message);
      }
    },

    /**
     * Register an inbound message listener.
     **/
    subscribe(callback) {
      listeners.push(callback);
      return () => {
        const idx = listeners.indexOf(callback);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },

    /**
     * Register a close listener.
     **/
    subscribeClose(callback) {
      if (closedError) {
        callback(closedError);
        return () => {};
      }

      closeListeners.push(callback);
      return () => {
        const idx = closeListeners.indexOf(callback);
        if (idx >= 0) closeListeners.splice(idx, 1);
      };
    },

    /**
     * Dispose the provider and close the port if it has resolved.
     **/
    dispose() {
      notifyClose(new Error("message port provider disposed"));
      try {
        resolvedPort?.close();
      } catch {
        // ignore duplicate close during shutdown
      }
      listeners.length = 0;
      closeListeners.length = 0;
    },
  };
}
