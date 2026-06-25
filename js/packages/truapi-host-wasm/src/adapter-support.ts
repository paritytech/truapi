// Hand-written runtime support for the generated `createWasmRawCallbacks`
// adapter (`./generated/host-callbacks-adapter.ts`). The adapter is mechanical
// (decode params, call the typed host callback, read the result); the pieces
// here are the genuinely bespoke runtime plumbing it leans on: stream driving
// and the chain-connection handle.

import { type GenericError, type Result } from "@parity/truapi";
import { hexToBytes } from "@parity/truapi/scale";

import type { ChainConnect, ChainConnection } from "./runtime.js";
import type { HostCallbacks } from "./generated/host-callbacks.js";

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

function pumpIterator<T>(
  iterator: AsyncIterator<T>,
  onItem: (value: T) => void,
  label: string,
): () => void {
  let stopped = false;
  void (async () => {
    try {
      while (!stopped) {
        const next = await iterator.next();
        if (next.done) return;
        onItem(next.value);
      }
    } catch (err) {
      console.error(`[truapi host callbacks] ${label} failed:`, err);
    }
  })();
  return () => {
    stopped = true;
    void iterator.return?.();
  };
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
  return pumpIterator(
    toAsyncIterator(stream),
    (value) => sendItem(unwrapStreamResult(value)),
    "subscription",
  );
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
    const stopResponses = pumpIterator(iterator, onResponse, "chain responses");
    return {
      send(request: string): void {
        connection.send(request);
      },
      close(): void {
        stopResponses();
      },
    };
  };
}
