import { errAsync, okAsync, ResultAsync } from "neverthrow";

import {
  decodeWireMessage,
  encodeWireMessage,
  type ProtocolMessage,
  type RequestFrameIds,
  type RequestParams,
  type SubscriptionFrameIds,
  type SubscribeRawParams,
  type Subscription,
  type TrUApiTransport,
  type WireProvider,
} from "./transport.js";
import {
  indexedTaggedUnion,
  Result,
  _void,
  type Codec,
  type ResultPayload,
} from "./scale.js";
import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";

export type { Subscription, TrUApiTransport };

/**
 * Version overrides used when constructing a transport.
 */
export interface CreateTransportOptions {
  /**
   * SCALE codec version advertised during host handshake negotiation.
   *
   * @deprecated TODO(shared-core-wire): remove this override with
   * `TrUApiTransport.codecVersion` once generated handshake requests use
   * `TRUAPI_CODEC_VERSION` directly.
   */
  codecVersion?: number;
}

/**
 * Convert a positive protocol version number into the generated version tag
 * used by TrUAPI wire wrappers.
 */
function protocolVersionTag(version: number): `V${number}` {
  if (!Number.isInteger(version) || version < 1) {
    throw new Error(`Invalid TrUAPI protocol version: ${version}`);
  }
  return `V${version}` as `V${number}`;
}

type HandshakeResponse = ResultPayload<undefined, T.HostHandshakeError>;
const HANDSHAKE_WIRE_VERSION = 1;

/**
 * Build the versioned handshake response codec for the selected wire version.
 */
function handshakeResponseCodec(
  version: number,
): Codec<{ tag: `V${number}`; value: HandshakeResponse }> {
  return indexedTaggedUnion({
    [protocolVersionTag(version)]: [
      version - 1,
      Result(_void, T.HostHandshakeError),
    ] as const,
  }) as Codec<{ tag: `V${number}`; value: HandshakeResponse }>;
}

/**
 * Encode a successful host-handshake response payload.
 */
function encodeSuccessfulHandshakeResponse(version: number): Uint8Array {
  return encodeHandshakeResponse(version, {
    tag: protocolVersionTag(version),
    value: {
      success: true,
      value: undefined,
    },
  });
}

/**
 * Encode a host-handshake response that reports an unsupported codec version.
 */
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

/**
 * Encode a typed handshake response with the versioned response codec.
 */
function encodeHandshakeResponse(
  version: number,
  response: { tag: `V${number}`; value: HandshakeResponse },
): Uint8Array {
  return handshakeResponseCodec(version).enc(response);
}

type VersionedWireValue = { tag: `V${number}`; value: unknown };

/**
 * Check whether a decoded SCALE value has the generated `{ tag, value }`
 * wrapper shape used for versioned wire payloads.
 */
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

/**
 * Return the inner payload from a versioned wire wrapper, or the original
 * value when the payload is already unwrapped.
 */
function unwrapVersionedWireValue(value: unknown): unknown {
  return isVersionedWireValue(value) ? value.value : value;
}

/**
 * Build a `TrUApiTransport` on top of a `WireProvider`, adding request/response
 * correlation and subscription start/receive/stop lifecycle handling.
 */
export function createTransport(
  provider: WireProvider,
  options: CreateTransportOptions = {},
): TrUApiTransport {
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

  /**
   * Normalize arbitrary thrown values into `Error` instances.
   */
  function toError(error: unknown): Error {
    return error instanceof Error ? error : new Error(String(error));
  }

  /**
   * Close the transport once, rejecting pending requests and notifying live
   * subscriptions.
   */
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

    if (payload.id === W.SYSTEM_HANDSHAKE.request) {
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
            id: W.SYSTEM_HANDSHAKE.response,
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

  /**
   * Encode and post a protocol message through the underlying provider.
   */
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
    codecVersion,
    /**
     * Send one request frame and resolve with the typed Ok/Err outcome
     * decoded from the response payload's `ResultPayload` envelope.
     */
    request<Ok, Err>({
      ids,
      payload,
      decodeResponse,
    }: RequestParams<Ok, Err>): ResultAsync<Ok, Err> {
      const promise = new Promise<ResultPayload<Ok, Err>>((resolve, reject) => {
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
      return ResultAsync.fromSafePromise(promise).andThen(
        (result): ResultAsync<Ok, Err> =>
          result.success ? okAsync(result.value) : errAsync(result.value),
      );
    },
    /**
     * Start a raw subscription and route incoming receive/interrupt frames to
     * the supplied callbacks.
     */
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
                value: _void.enc(undefined),
              },
            });
          } catch {
            // provider already closed
          }
        },
      };
    },
    /**
     * Close this transport and detach its provider listeners.
     */
    dispose() {
      // Idempotent: closeWithError is a no-op once closedError is set, and
      // unsubscribe handles tolerate being called twice.
      closeWithError(new Error("transport disposed"));
      unsubscribeMessage();
      unsubscribeClose?.();
    },
  };
}
