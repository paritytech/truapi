import {
  decodeWireMessage,
  encodeWireMessage,
  type Provider,
  type ProtocolMessage,
  type RequestFrameIds,
  type RequestParams,
  type SubscriptionFrameIds,
  type SubscribeRawParams,
  type Subscription,
  type TrUApiTransport,
} from "./transport.js";
import {
  indexedTaggedUnion,
  result,
  unit,
  type Codec,
  type ResultPayload,
} from "./scale.js";
import { TRUAPI_CODEC_VERSION, TRUAPI_VERSION } from "./generated/client.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";

export type { Subscription, TrUApiTransport };

export interface CreateTransportOptions {
  truapiVersion?: number;
  codecVersion?: number;
}

function protocolVersionTag(version: number): `V${number}` {
  if (!Number.isInteger(version) || version < 1) {
    throw new Error(`Invalid TrUAPI protocol version: ${version}`);
  }
  return `V${version}` as `V${number}`;
}

type HandshakeResponse = ResultPayload<undefined, T.HostHandshakeError>;
const HANDSHAKE_WIRE_VERSION = 1;

function handshakeResponseCodec(
  version: number,
): Codec<{ tag: `V${number}`; value: HandshakeResponse }> {
  return indexedTaggedUnion({
    [protocolVersionTag(version)]: [
      version - 1,
      result(unit, T.HostHandshakeError),
    ] as const,
  }) as Codec<{ tag: `V${number}`; value: HandshakeResponse }>;
}

function encodeSuccessfulHandshakeResponse(version: number): Uint8Array {
  return encodeHandshakeResponse(version, {
    tag: protocolVersionTag(version),
    value: {
      success: true,
      value: undefined,
    },
  });
}

function encodeUnsupportedHandshakeResponse(version: number): Uint8Array {
  return encodeHandshakeResponse(version, {
    tag: protocolVersionTag(version),
    value: {
      success: false,
      value: {
        tag: "UnsupportedProtocolVersion",
        value: undefined,
      },
    },
  });
}

function encodeHandshakeResponse(
  version: number,
  response: { tag: `V${number}`; value: HandshakeResponse },
): Uint8Array {
  return handshakeResponseCodec(version).enc(response);
}

type VersionedWireValue = { tag: `V${number}`; value: unknown };

function isVersionedWireValue(value: unknown): value is VersionedWireValue {
  return (
    typeof value === "object" &&
    value !== null &&
    "tag" in value &&
    "value" in value &&
    typeof value.tag === "string" &&
    /^V\d+$/.test(value.tag)
  );
}

function unwrapVersionedWireValue(value: unknown): unknown {
  return isVersionedWireValue(value) ? value.value : value;
}

/** Build a TrUApiTransport on top of a Provider (request/response correlation,
 * subscription start/receive/stop lifecycle). */
export function createTransport(
  provider: Provider,
  options: CreateTransportOptions = {},
): TrUApiTransport {
  const truapiVersion = options.truapiVersion ?? TRUAPI_VERSION;
  const codecVersion = options.codecVersion ?? TRUAPI_CODEC_VERSION;
  let idCounter = 0;
  let closedError: Error | null = null;
  const pending = new Map<
    string,
    {
      ids: RequestFrameIds;
      resolve: (value: Uint8Array) => void;
      reject: (error: Error) => void;
    }
  >();
  const subscriptions = new Map<
    string,
    {
      ids: SubscriptionFrameIds;
      onReceive: (payload: Uint8Array) => void;
      onInterrupt?: (payload: Uint8Array) => void;
      onClose?: (error: Error) => void;
    }
  >();

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
      subscription.onClose?.(nextError);
    }
  }

  const unsubscribeClose = provider.subscribeClose?.((error) => {
    closeWithError(error);
  });

  const unsubscribeMessage = provider.subscribe((message) => {
    if (closedError) {
      return;
    }

    const decoded = decodeWireMessage(message);
    if (decoded.isErr()) {
      closeWithError(decoded.error);
      return;
    }
    const { requestId, payload } = decoded.value;

    if (payload.id === W.HOST_HANDSHAKE.request) {
      // Auto-respond to inbound `host_handshake_request` frames.
      //
      // Legacy hosts shipping `@novasamatech/host-api@0.6.x` (e.g. dotli)
      // initiate their own handshake from the host side at startup and ping
      // the iframe with `host_handshake_request` every 50ms until they see a
      // matching response. The legacy host-api `createTransport` registered
      // an internal handler for this message; preserving that behaviour
      // keeps `@parity/truapi` a drop-in replacement for legacy bridges.
      //
      // Respond with the handshake method's selected wire version. The inner
      // request carries the wire codec version.
      let response: Uint8Array;
      try {
        const request = unwrapVersionedWireValue(
          T.VersionedHostHandshakeRequest.dec(payload.value),
        ) as T.HostHandshakeRequest;
        const requestedCodecVersion = request.codecVersion;
        response =
          requestedCodecVersion === codecVersion
            ? encodeSuccessfulHandshakeResponse(HANDSHAKE_WIRE_VERSION)
            : encodeUnsupportedHandshakeResponse(HANDSHAKE_WIRE_VERSION);
      } catch (error) {
        closeWithError(toError(error));
        return;
      }
      try {
        send({
          requestId,
          payload: {
            id: W.HOST_HANDSHAKE.response,
            value: response,
          },
        });
      } catch {
        // provider already closed
      }
      return;
    }

    const p = pending.get(requestId);
    if (p) {
      if (payload.id !== p.ids.response) {
        return;
      }
      pending.delete(requestId);
      try {
        p.resolve(payload.value);
      } catch (error) {
        p.reject(toError(error));
      }
      return;
    }

    const subscription = subscriptions.get(requestId);
    if (subscription) {
      if (payload.id === subscription.ids.receive) {
        try {
          subscription.onReceive(payload.value);
        } catch (error) {
          // A consumer-side decode/handler error must not tear down the
          // provider's message loop and silently break every other
          // subscription on the same transport. Surface via onClose and
          // drop this subscription; siblings stay alive.
          subscriptions.delete(requestId);
          subscription.onClose?.(toError(error));
        }
      } else if (payload.id === subscription.ids.interrupt) {
        subscriptions.delete(requestId);
        subscription.onInterrupt?.(payload.value);
      }
    }
  });

  function send(message: ProtocolMessage) {
    if (closedError) {
      throw closedError;
    }

    const encoded = encodeWireMessage(message);
    if (encoded.isErr()) {
      closeWithError(encoded.error);
      throw encoded.error;
    }

    try {
      provider.postMessage(encoded.value);
    } catch (error) {
      closeWithError(error);
      throw toError(error);
    }
  }

  return {
    truapiVersion,
    codecVersion,
    request<Response>({
      ids,
      payload,
      decodeResponse,
    }: RequestParams<Response>) {
      return new Promise<Response>((resolve, reject) => {
        if (closedError) {
          reject(closedError);
          return;
        }

        const requestId = `p:${++idCounter}`;
        pending.set(requestId, {
          ids,
          resolve: (response) => resolve(decodeResponse(response)),
          reject,
        });
        try {
          send({
            requestId,
            payload: {
              id: ids.request,
              value: payload,
            },
          });
        } catch (error) {
          pending.delete(requestId);
          reject(toError(error));
        }
      });
    },
    subscribeRaw({
      ids,
      payload,
      onReceive,
      onInterrupt,
      onClose,
    }: SubscribeRawParams) {
      if (closedError) {
        onClose?.(closedError);
        return { unsubscribe: () => {}, subscriptionId: "" };
      }

      const requestId = `p:${++idCounter}`;
      subscriptions.set(requestId, {
        ids,
        onReceive,
        onInterrupt,
        onClose,
      });
      try {
        send({
          requestId,
          payload: {
            id: ids.start,
            value: payload,
          },
        });
      } catch (error) {
        subscriptions.delete(requestId);
        onClose?.(toError(error));
        return { unsubscribe: () => {}, subscriptionId: requestId };
      }
      return {
        subscriptionId: requestId,
        unsubscribe: () => {
          // Skip the `_stop` frame when the host already terminated the stream
          // via `_interrupt` (which removes the entry from `subscriptions`).
          if (!subscriptions.has(requestId)) return;
          subscriptions.delete(requestId);
          try {
            send({
              requestId,
              payload: {
                id: ids.stop,
                value: unit.enc(undefined),
              },
            });
          } catch {
            // provider already closed
          }
        },
      };
    },
    dispose() {
      // Idempotent: closeWithError is a no-op once closedError is set, and
      // unsubscribe handles tolerate being called twice.
      closeWithError(new Error("transport disposed"));
      unsubscribeMessage();
      unsubscribeClose?.();
    },
  };
}
