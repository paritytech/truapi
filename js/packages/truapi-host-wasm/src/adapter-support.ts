// Hand-written runtime support for the generated `createWasmRawCallbacks`
// adapter (`./generated/host-callbacks-adapter.ts`). The adapter is mechanical
// (decode params, call the typed host callback, read the result); the pieces
// here are the genuinely bespoke runtime plumbing it leans on: stream driving
// and the chain-connection handle.

import {
  HostDevicePermissionResponse,
  HostPushNotificationResponse,
  RemotePermissionResponse,
  ThemeVariant,
  type GenericError,
  type Result,
} from "@parity/truapi";
import { hexToBytes } from "@parity/truapi/scale";

import type { ChainConnect, ChainConnection, HostCallbacks } from "./runtime.js";
import type { RawCallbacks } from "./generated/host-callbacks-adapter.js";

type WireResult<T, E> =
  | { success: true; value: T }
  | { success: false; value: E };

type StreamResult<T, E> = Result<T, E> | WireResult<T, E>;

type MaybeAsyncIterable<T> = AsyncIterable<T> | Iterable<T>;

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

/**
 * Drive a typed host stream of `Result` items into the core's `sendItem`
 * sink, unwrapping each `Result` (or throwing on its error). Returns a
 * disposer that stops iteration.
 */
export function driveResultStream<T>(
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
      console.error("[truapi host callbacks] subscription failed:", err);
    }
  })();
  return () => {
    stopped = true;
    void iterator.return?.();
  };
}

/**
 * Bridge the typed `ChainProvider.connect` callback onto the raw
 * `chainConnect` the WASM core invokes: decode the genesis hash, pump the
 * connection's `responses()` stream into `onResponse`, and expose
 * `send`/`close`.
 */
export function chainConnectAdapter(
  host: Partial<HostCallbacks>,
): ChainConnect | undefined {
  if (!host.connect) return undefined;
  return async (genesisHash, onResponse): Promise<ChainConnection | null> => {
    const connection = await host.connect!(hexToBytes(genesisHash));
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
        console.error("[truapi host callbacks] chain responses failed:", err);
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
 * Defaults for every callback, used by the generated adapter when the host
 * does not implement one. These reproduce the core's absent-callback
 * semantics: permissions deny, notifications no-op, storage/session read
 * empty, confirmations deny, required capabilities throw, and subscriptions
 * emit a single current default. Codec-typed results are SCALE-encoded to
 * match the symmetric callback boundary.
 */
export function createUnavailableCallbacks(): Omit<RawCallbacks, "chainConnect"> {
  const unavailable = (method: string) => async (): Promise<never> => {
    throw new Error(`${method} unavailable on this host`);
  };
  return {
    navigateTo: unavailable("navigateTo"),
    pushNotification: async () => HostPushNotificationResponse.enc({ id: 0 }),
    cancelNotification: async () => {},
    devicePermission: async () =>
      HostDevicePermissionResponse.enc({ granted: false }),
    remotePermission: async () =>
      RemotePermissionResponse.enc({ granted: false }),
    featureSupported: unavailable("featureSupported"),
    read: async () => undefined,
    write: unavailable("write"),
    clear: unavailable("clear"),
    authStateChanged: () => {},
    readSession: async () => undefined,
    writeSession: async () => {},
    clearSession: async () => {},
    subscribeSessionStore: (sendItem) => {
      sendItem();
    },
    confirmSignPayload: async () => false,
    confirmSignRaw: async () => false,
    confirmCreateTransaction: async () => false,
    confirmAccountAlias: async () => false,
    confirmResourceAllocation: async () => false,
    confirmPreimageSubmit: unavailable("confirmPreimageSubmit"),
    submitPreimage: unavailable("submitPreimage"),
    subscribeTheme: (sendItem) => {
      sendItem(ThemeVariant.enc("Dark"));
    },
    lookupPreimage: (_key, sendItem) => {
      sendItem(undefined);
    },
  };
}
