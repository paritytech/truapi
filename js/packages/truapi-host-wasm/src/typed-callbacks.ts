import {
  HostDevicePermissionRequest,
  HostFeatureSupportedRequest,
  HostPushNotificationRequest,
  RemotePermissionRequest,
  ThemeVariant,
  type GenericError,
  type Result,
} from "@parity/truapi";
import { hexToBytes } from "@parity/truapi/scale";

import {
  createUnavailableCallbacks,
  type ChainConnection,
  type WasmRawCallbacks,
} from "./runtime.js";
import type {
  HostCallbacks,
  SessionUiInfo,
} from "./generated/host-callbacks.js";

type WireResult<T, E> =
  | { success: true; value: T }
  | { success: false; value: E };

type StreamResult<T, E> = Result<T, E> | WireResult<T, E>;

type MaybeAsyncIterable<T> = AsyncIterable<T> | Iterable<T>;

type OptionalTypedCallbacks = Partial<HostCallbacks>;

type RawWithoutEmit = Omit<WasmRawCallbacks, "emitFrame">;

const decodePushNotification = HostPushNotificationRequest.dec;
const decodeDevicePermission = HostDevicePermissionRequest.dec;
const decodeRemotePermission = RemotePermissionRequest.dec;
const decodeFeatureSupported = HostFeatureSupportedRequest.dec;

function errorReason(error: GenericError): string {
  return error.reason;
}

function unwrapStreamResult<T>(item: StreamResult<T, GenericError>): T {
  if ("success" in item) {
    if (item.success === false) {
      throw new Error(errorReason(item.value));
    }
    return item.value;
  }
  if (item.isErr()) {
    throw new Error(errorReason(item.error));
  }
  return item.value;
}

function toAsyncIterator<T>(stream: MaybeAsyncIterable<T>): AsyncIterator<T> {
  const asyncIterable = stream as AsyncIterable<T>;
  if (typeof asyncIterable[Symbol.asyncIterator] === "function") {
    return asyncIterable[Symbol.asyncIterator]();
  }

  const iterator = (stream as Iterable<T>)[Symbol.iterator]();
  const asyncIterator: AsyncIterator<T> = {
    next: async () => iterator.next(),
  };
  if (iterator.return) {
    asyncIterator.return = async () => iterator.return!();
  }
  return asyncIterator;
}

function driveResultStream<T>(
  stream: MaybeAsyncIterable<StreamResult<T, GenericError>>,
  sendItem: (value: T) => void,
): () => void {
  const iterator = toAsyncIterator(stream);
  let stopped = false;
  void (async () => {
    try {
      while (!stopped) {
        const next = await iterator.next();
        if (next.done) return;
        sendItem(unwrapStreamResult(next.value));
      }
    } catch (err) {
      console.error("[truapi typed callbacks] subscription failed:", err);
    }
  })();
  return () => {
    stopped = true;
    void iterator.return?.();
  };
}

function chainConnect(
  callbacks: OptionalTypedCallbacks,
): RawWithoutEmit["chainConnect"] {
  if (!callbacks.connect) return undefined;
  return async (genesisHash, onResponse): Promise<ChainConnection | null> => {
    const connection = await callbacks.connect!(hexToBytes(genesisHash));
    const iterator = connection.responses()[Symbol.asyncIterator]();
    let closed = false;
    void (async () => {
      try {
        while (!closed) {
          const next = await iterator.next();
          if (next.done) return;
          onResponse(next.value);
        }
      } catch (err) {
        console.error("[truapi typed callbacks] chain responses failed:", err);
      }
    })();
    return {
      send(request: string): void {
        connection.send(request);
      },
      close(): void {
        closed = true;
        void iterator.return?.();
      },
    };
  };
}

/**
 * Adapt generated typed host callbacks into the raw SCALE-byte callback
 * surface consumed by the WASM core.
 */
export function createWasmRawCallbacks(
  callbacks: OptionalTypedCallbacks,
): RawWithoutEmit {
  const unavailable = createUnavailableCallbacks();
  const connect = chainConnect(callbacks);
  return {
    ...unavailable,
    navigateTo: callbacks.navigateTo
      ? (url) => callbacks.navigateTo!(url)
      : unavailable.navigateTo,
    pushNotification: callbacks.pushNotification
      ? async (payload) => {
          const response = await callbacks.pushNotification!(
            decodePushNotification(payload),
          );
          return response.id;
        }
      : unavailable.pushNotification,
    ...(callbacks.cancelNotification
      ? {
          cancelNotification: (id: number) => callbacks.cancelNotification!(id),
        }
      : {}),
    devicePermission: callbacks.devicePermission
      ? async (payload) => {
          const response = await callbacks.devicePermission!(
            decodeDevicePermission(payload),
          );
          return response.granted;
        }
      : unavailable.devicePermission,
    remotePermission: callbacks.remotePermission
      ? async (payload) => {
          const response = await callbacks.remotePermission!(
            decodeRemotePermission(payload),
          );
          return response.granted;
        }
      : unavailable.remotePermission,
    featureSupported: callbacks.featureSupported
      ? async (payload) => {
          const response = await callbacks.featureSupported!(
            decodeFeatureSupported(payload),
          );
          return response.supported;
        }
      : unavailable.featureSupported,
    localStorageRead: callbacks.read
      ? (key) => callbacks.read!(key)
      : unavailable.localStorageRead,
    localStorageWrite: callbacks.write
      ? (key, value) => callbacks.write!(key, value)
      : unavailable.localStorageWrite,
    localStorageClear: callbacks.clear
      ? (key) => callbacks.clear!(key)
      : unavailable.localStorageClear,
    ...(callbacks.presentPairing
      ? {
          presentPairing: (deeplink: string) =>
            callbacks.presentPairing!(deeplink),
        }
      : {}),
    ...(callbacks.readSession
      ? { readSession: () => callbacks.readSession!() }
      : {}),
    ...(callbacks.writeSession
      ? { writeSession: (value: Uint8Array) => callbacks.writeSession!(value) }
      : {}),
    ...(callbacks.clearSession
      ? { clearSession: () => callbacks.clearSession!() }
      : {}),
    ...(callbacks.subscribeSessionStore
      ? {
          subscribeSessionStore: (sendItem: () => void) =>
            driveResultStream(callbacks.subscribeSessionStore!(), () =>
              sendItem(),
            ),
        }
      : {}),
    ...(callbacks.sessionUiChanged
      ? {
          sessionUiChanged: (info: SessionUiInfo) =>
            callbacks.sessionUiChanged!(info),
        }
      : {}),
    ...(callbacks.confirmSignPayload
      ? {
          confirmSignPayload: (payload: Uint8Array) =>
            callbacks.confirmSignPayload!(payload),
        }
      : {}),
    ...(callbacks.confirmSignRaw
      ? {
          confirmSignRaw: (payload: Uint8Array) =>
            callbacks.confirmSignRaw!(payload),
        }
      : {}),
    ...(callbacks.confirmCreateTransaction
      ? {
          confirmCreateTransaction: (payload: Uint8Array) =>
            callbacks.confirmCreateTransaction!(payload),
        }
      : {}),
    ...(callbacks.confirmAccountAlias
      ? {
          confirmAccountAlias: (payload: Uint8Array) =>
            callbacks.confirmAccountAlias!(payload),
        }
      : {}),
    ...(callbacks.confirmResourceAllocation
      ? {
          confirmResourceAllocation: (payload: Uint8Array) =>
            callbacks.confirmResourceAllocation!(payload),
        }
      : {}),
    confirmPreimageSubmit: callbacks.confirmPreimageSubmit
      ? (size) => callbacks.confirmPreimageSubmit!(BigInt(size))
      : unavailable.confirmPreimageSubmit,
    submitPreimage: callbacks.submitPreimage
      ? (value) => callbacks.submitPreimage!(value)
      : unavailable.submitPreimage,
    ...(callbacks.subscribeTheme
      ? {
          themeSubscribe: (sendItem: (theme: ThemeVariant) => void) =>
            driveResultStream(callbacks.subscribeTheme!(), sendItem),
        }
      : {}),
    preimageLookupSubscribe: callbacks.lookupPreimage
      ? (key, sendItem) =>
          driveResultStream(callbacks.lookupPreimage!(key), (item) => {
            const value = item as Uint8Array | undefined;
            sendItem(value);
          })
      : unavailable.preimageLookupSubscribe,
    ...(connect ? { chainConnect: connect } : {}),
  };
}
