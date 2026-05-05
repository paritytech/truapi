import {
  byteProtocolCodecAdapter,
  type CodecAdapter,
  type Provider,
  type ProtocolMessage,
} from './transport.js';
import { unit, type Codec } from './scale.js';

/** Handle returned by `subscribe`. `unsubscribe` is idempotent;
 * `subscriptionId` is the transport-assigned id for the subscribe frame, which
 * methods that take a `followSubscriptionId` need to scope their request to
 * this subscription. */
export interface Subscription {
  unsubscribe: () => void;
  subscriptionId: string;
}

/**
 * Transport used by generated client stubs. Typed values exist only at the
 * generated client boundary. Payloads are always SCALE-encoded bytes, while
 * the outer transport adapter decides whether the envelope travels as bytes
 * or as a structured-clone object containing those payload bytes.
 */
export interface TrUApiTransport {
  request<Request, Response>(
    method: string,
    value: Request,
    requestCodec: Codec<Request>,
    responseCodec: Codec<Response>,
  ): Promise<Response>;
  subscribe<Start, Item, Interrupt = never>(
    method: string,
    value: Start,
    startCodec: Codec<Start>,
    itemCodec: Codec<Item>,
    callback: (data: Item) => void,
    interruptCodec?: Codec<Interrupt>,
    onInterrupt?: (data: Interrupt) => void,
  ): Subscription;
}

// SCALE-encoded `Ok(_void)` wrapped in the `v1` versioned variant. Two bytes:
//   [0x00] — V1 enum discriminant
//   [0x00] — `Result::Ok` discriminant (no body for `_void`)
// The host-api decoder (legacy and current) reads these as a successful
// handshake regardless of how the trait happens to be spelled.
const HANDSHAKE_RESPONSE_V1_OK_BYTES: Uint8Array = new Uint8Array([0x00, 0x00]);

/** Build a TrUApiTransport on top of a Provider (request/response correlation,
 * subscription start/receive/stop lifecycle). */
export function createTransport(
  provider: Provider,
  codec: CodecAdapter = byteProtocolCodecAdapter,
): TrUApiTransport {
  let idCounter = 0;
  let closedError: Error | null = null;
  const pending = new Map<
    string,
    { resolve: (value: unknown) => void; reject: (error: Error) => void }
  >();
  const subscriptions = new Map<
    string,
    {
      callback: (data: unknown) => void;
      itemCodec: Codec<any>;
      interruptCodec?: Codec<any>;
      onInterrupt?: (data: unknown) => void;
    }
  >();

  function encodePayload<T>(value: T, payloadCodec: Codec<T>): Uint8Array {
    return payloadCodec.enc(value);
  }

  function decodePayload<T>(value: unknown, payloadCodec: Codec<T>): T {
    if (!(value instanceof Uint8Array)) {
      throw new Error(`Expected SCALE payload bytes, received ${describePayload(value)}`);
    }
    return payloadCodec.dec(value);
  }

  function describePayload(value: unknown): string {
    if (value === undefined) return 'undefined';
    if (value === null) return 'null';
    if (value instanceof Uint8Array) return `Uint8Array(${value.length})`;
    if (Array.isArray(value)) return 'Array';
    if (typeof value === 'object') return 'object';
    return typeof value;
  }

  function toError(error: unknown): Error {
    return error instanceof Error ? error : new Error(String(error));
  }

  function closeWithError(error: unknown) {
    const nextError = toError(error);
    if (closedError) {
      return;
    }

    closedError = nextError;

    for (const [requestId, entry] of pending) {
      pending.delete(requestId);
      entry.reject(nextError);
    }

    for (const [requestId, subscription] of subscriptions) {
      subscriptions.delete(requestId);
      subscription.onInterrupt?.(nextError);
    }
  }

  provider.subscribeClose?.((error) => {
    closeWithError(error);
  });

  provider.subscribe((message) => {
    if (closedError) {
      return;
    }

    const { requestId, payload } = codec.decode(message);

    if (payload.tag.endsWith('_response')) {
      const p = pending.get(requestId);
      if (p) {
        pending.delete(requestId);
        try {
          p.resolve(payload.value);
        } catch (error) {
          p.reject(toError(error));
        }
      }
    } else if (payload.tag.endsWith('_receive')) {
      const subscription = subscriptions.get(requestId);
      if (subscription) {
        subscription.callback(decodePayload(payload.value, subscription.itemCodec));
      }
    } else if (payload.tag.endsWith('_interrupt')) {
      const subscription = subscriptions.get(requestId);
      subscriptions.delete(requestId);
      if (subscription?.onInterrupt) {
        const interruptCodec = subscription.interruptCodec;
        if (!interruptCodec) {
          throw new Error(`Interrupt payload for ${payload.tag} is missing a codec`);
        }
        subscription.onInterrupt(
          decodePayload(payload.value, interruptCodec),
        );
      }
    } else if (payload.tag === 'host_handshake_request') {
      // Auto-respond to inbound `host_handshake_request` frames.
      //
      // Legacy hosts shipping `@novasamatech/host-api@0.6.x` (e.g. dotli)
      // initiate their own handshake from the host side at startup and ping
      // the iframe with `host_handshake_request` every 50ms until they see a
      // matching response. The legacy host-api `createTransport` registered
      // an internal handler for this message; preserving that behaviour
      // keeps `@truapi/client` a drop-in replacement for legacy bridges.
      //
      // Wire bytes match `Enum({v1: Result(_void, HandshakeErr)})::Ok` for
      // legacy decoders and `Result::Ok(HostHandshakeResponse::V1)` for
      // current decoders — both encode to `[0x00, 0x00]`.
      try {
        send({
          requestId,
          payload: { tag: 'host_handshake_response', value: HANDSHAKE_RESPONSE_V1_OK_BYTES },
        });
      } catch {
        // provider already closed
      }
    }
  });

  function send(message: ProtocolMessage) {
    if (closedError) {
      throw closedError;
    }

    try {
      provider.postMessage(codec.encode(message));
    } catch (error) {
      closeWithError(error);
      throw toError(error);
    }
  }

  return {
    request<Request, Response>(
      method: string,
      value: Request,
      requestCodec: Codec<Request>,
      responseCodec: Codec<Response>,
    ) {
      return new Promise<Response>((resolve, reject) => {
        if (closedError) {
          reject(closedError);
          return;
        }

        const requestId = `p:${++idCounter}`;
        pending.set(requestId, {
          resolve: (response) => resolve(decodePayload(response, responseCodec)),
          reject,
        });
        try {
          send({
            requestId,
            payload: {
              tag: `${method}_request`,
              value: encodePayload(value, requestCodec),
            },
          });
        } catch (error) {
          pending.delete(requestId);
          reject(toError(error));
        }
      });
    },
    subscribe<Start, Item, Interrupt = never>(
      method: string,
      value: Start,
      startCodec: Codec<Start>,
      itemCodec: Codec<Item>,
      callback: (data: Item) => void,
      interruptCodec?: Codec<Interrupt>,
      onInterrupt?: (data: Interrupt) => void,
    ) {
      if (closedError) {
        onInterrupt?.(closedError as Interrupt);
        return { unsubscribe: () => {}, subscriptionId: '' };
      }

      const requestId = `p:${++idCounter}`;
      subscriptions.set(requestId, {
        callback: (data) => callback(data as Item),
        itemCodec,
        interruptCodec,
        onInterrupt: onInterrupt ? (data) => onInterrupt(data as Interrupt) : undefined,
      });
      try {
        send({
          requestId,
          payload: {
            tag: `${method}_start`,
            value: encodePayload(value, startCodec),
          },
        });
      } catch (error) {
        subscriptions.delete(requestId);
        onInterrupt?.(toError(error) as Interrupt);
        return { unsubscribe: () => {}, subscriptionId: requestId };
      }
      return {
        subscriptionId: requestId,
        unsubscribe: () => {
          subscriptions.delete(requestId);
          try {
            send({
              requestId,
              payload: {
                tag: `${method}_stop`,
                value: encodePayload(undefined, unit),
              },
            });
          } catch {
            // provider already closed
          }
        },
      };
    },
  };
}
