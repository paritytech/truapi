import {
  decodeWireMessage,
  encodeWireMessage,
  type Provider,
  type ProtocolMessage,
  type RequestParams,
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
    { resolve: (value: Uint8Array) => void; reject: (error: Error) => void }
  >();
  const subscriptions = new Map<
    string,
    {
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

  provider.subscribeClose?.((error) => {
    closeWithError(error);
  });

  provider.subscribe((message) => {
    if (closedError) {
      return;
    }

    const decoded = decodeWireMessage(message);
    if (decoded.isErr()) {
      closeWithError(decoded.error);
      return;
    }
    const { requestId, payload } = decoded.value;

    if (payload.tag.endsWith("_response")) {
      const p = pending.get(requestId);
      if (p) {
        pending.delete(requestId);
        try {
          p.resolve(payload.value);
        } catch (error) {
          p.reject(toError(error));
        }
      }
    } else if (payload.tag.endsWith("_receive")) {
      const subscription = subscriptions.get(requestId);
      if (subscription) {
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
      }
    } else if (payload.tag.endsWith("_interrupt")) {
      const subscription = subscriptions.get(requestId);
      subscriptions.delete(requestId);
      subscription?.onInterrupt?.(payload.value);
    } else if (payload.tag === "host_handshake_request") {
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
            tag: "host_handshake_response",
            value: response,
          },
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
      method,
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
          resolve: (response) => resolve(decodeResponse(response)),
          reject,
        });
        try {
          send({
            requestId,
            payload: {
              tag: `${method}_request`,
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
      method,
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
        onReceive,
        onInterrupt,
        onClose,
      });
      try {
        send({
          requestId,
          payload: {
            tag: `${method}_start`,
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
                tag: `${method}_stop`,
                value: unit.enc(undefined),
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
