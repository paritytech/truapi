import { describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";

import {
  HostDevicePermissionRequest,
  HostDevicePermissionResponse,
  HostFeatureSupportedRequest,
  HostFeatureSupportedResponse,
  HostPushNotificationRequest,
  HostPushNotificationResponse,
  RemotePermissionRequest,
  RemotePermissionResponse,
  ThemeVariant,
} from "@parity/truapi";
import type { GenericError, HostSignPayloadData } from "@parity/truapi";

import { createWasmRawCallbacks } from "./generated/host-callbacks-adapter.js";
import {
  AuthState,
  CoreStorageKey,
  UserConfirmationReview,
} from "./generated/host-callbacks.js";
import { makeHostCallbacks, settle } from "./test-support.js";

// The generated `createWasmRawCallbacks` adapter speaks the symmetric SCALE
// byte boundary: codec-typed requests arrive as `Uint8Array` and are decoded
// for the typed host callback; codec-typed responses are SCALE-encoded back to
// `Uint8Array`. Primitives, strings and byte blobs pass through unchanged.

const GENESIS = `0x${"11".repeat(32)}` as `0x${string}`;
const PRODUCT_ACCOUNT = {
  dotNsIdentifier: "playground.dot",
  derivationIndex: { tag: "Left" as const, value: 0 },
};
const PROOF_CONTEXT = {
  productId: "playground.dot",
  suffix: { tag: "Left" as const, value: 0 },
};
const RING_LOCATION = {
  chainId: GENESIS,
  junctions: [{ tag: "PalletInstance" as const, value: 67 }],
};
const SIGN_PAYLOAD: HostSignPayloadData = {
  blockHash: GENESIS,
  blockNumber: "0x01",
  era: "0x00",
  genesisHash: GENESIS,
  method: "0x0102",
  nonce: "0x00",
  specVersion: "0x01",
  tip: "0x00",
  transactionVersion: "0x01",
  signedExtensions: [],
  version: 4,
  assetId: undefined,
  metadataHash: undefined,
  mode: undefined,
};

describe("createWasmRawCallbacks", () => {
  it("decodes requests and encodes typed responses", async () => {
    const writes: [string, number[]][] = [];
    const clears: string[] = [];
    const cancelled: number[] = [];
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        notifications: {
          pushNotification: async (notification) => ({
            id: notification.text.length,
          }),
          cancelNotification: async (id) => {
            cancelled.push(id);
          },
        },
        permissions: {
          devicePermission: async (request) => ({
            granted: request === "Camera",
          }),
          remotePermission: async (request) => ({
            granted: request.permission.tag === "ChainSubmit",
          }),
        },
        features: {
          featureSupported: async (request) => ({
            supported:
              request.tag === "Chain" && request.value.genesisHash === GENESIS,
          }),
        },
        productStorage: {
          read: async (key) => new TextEncoder().encode(`read:${key}`),
          write: async (key, value) => {
            writes.push([key, [...value]]);
          },
          clear: async (key) => {
            clears.push(key);
          },
        },
      }),
    );

    expect(
      HostPushNotificationResponse.dec(
        await raw.pushNotification!(
          HostPushNotificationRequest.enc({
            text: "hello",
            deeplink: undefined,
            scheduledAt: undefined,
          }),
        ),
      ).id,
    ).toBe(5);
    expect(
      HostDevicePermissionResponse.dec(
        await raw.devicePermission!(HostDevicePermissionRequest.enc("Camera")),
      ).granted,
    ).toBe(true);
    expect(
      RemotePermissionResponse.dec(
        await raw.remotePermission!(
          RemotePermissionRequest.enc({
            permission: { tag: "ChainSubmit" },
          }),
        ),
      ).granted,
    ).toBe(true);
    expect(
      HostFeatureSupportedResponse.dec(
        await raw.featureSupported!(
          HostFeatureSupportedRequest.enc({
            tag: "Chain",
            value: { genesisHash: GENESIS },
          }),
        ),
      ).supported,
    ).toBe(true);
    expect(await raw.read!("session")).toEqual(
      new TextEncoder().encode("read:session"),
    );

    await raw.write!("session", new Uint8Array([1, 2, 3]));
    await raw.clear!("session");
    await raw.cancelNotification?.(9);

    expect(writes).toEqual([["session", [1, 2, 3]]]);
    expect(clears).toEqual(["session"]);
    expect(cancelled).toEqual([9]);
  });

  it("bridges lifecycle, confirmations, and preimage callbacks", async () => {
    const calls: unknown[][] = [];
    async function* preimages() {
      yield ok(undefined);
      yield ok(new Uint8Array([4, 5, 6]));
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        auth: {
          authStateChanged: (state) => {
            calls.push(["authStateChanged", state]);
          },
        },
        coreStorage: {
          readCoreStorage: async (key) =>
            key.tag === "AuthSession" ? new Uint8Array([1, 2, 3]) : undefined,
          writeCoreStorage: async (key, value) => {
            calls.push(["writeCoreStorage", key, [...value]]);
          },
          clearCoreStorage: async (key) => {
            calls.push(["clearCoreStorage", key]);
          },
        },
        userConfirmation: {
          confirmUserAction: async (review) => {
            switch (review.tag) {
              case "SignPayload":
                return (
                  review.value.tag === "Product" &&
                  review.value.value.account.dotNsIdentifier ===
                    "playground.dot" &&
                  review.value.value.payload.method === "0x0102"
                );
              case "SignRaw":
                return (
                  review.value.tag === "Product" &&
                  review.value.value.payload.tag === "Bytes" &&
                  review.value.value.payload.value.bytes === "0x0304"
                );
              case "CreateTransaction":
                return (
                  review.value.tag === "Product" &&
                  review.value.value.signer.derivationIndex.tag === "Left" &&
                  review.value.value.callData === "0x0506"
                );
              case "AccountAlias":
                return (
                  review.value.callingProductId === "playground.dot" &&
                  review.value.context.productId === "playground.dot" &&
                  review.value.ringLocation.junctions[0]?.tag ===
                    "PalletInstance"
                );
              case "CreateProof":
                return (
                  review.value.callingProductId === "playground.dot" &&
                  review.value.context.suffix.tag === "Left" &&
                  review.value.message[0] === 7
                );
              case "AccountAccess":
                return (
                  review.value.requestingProductId === "playground.dot" &&
                  review.value.targetProductId === "wallet.dot"
                );
              case "ResourceAllocation":
                return (
                  review.value.resources[0]?.tag === "StatementStoreAllowance"
                );
              case "PreimageSubmit":
                calls.push([
                  "confirmUserAction:PreimageSubmit",
                  review.value.size,
                ]);
                return review.value.size === 42n;
            }
          },
        },
        preimage: {
          lookupPreimage: (key) => {
            calls.push(["lookupPreimage", [...key]]);
            return preimages();
          },
        },
      }),
    );

    const preimageEvents: (number[] | null)[] = [];
    const disposePreimages = raw.lookupPreimage!(new Uint8Array([9]), (value) =>
      preimageEvents.push(value ? [...value] : null),
    );

    raw.authStateChanged?.(
      AuthState.enc({
        tag: "Pairing",
        value: { deeplink: "polkadotapp://example" },
      }),
    );
    const authSessionKey = CoreStorageKey.enc({ tag: "AuthSession" });
    expect(await raw.readCoreStorage!(authSessionKey)).toEqual(
      new Uint8Array([1, 2, 3]),
    );
    await raw.writeCoreStorage!(authSessionKey, new Uint8Array([3, 2, 1]));
    await raw.clearCoreStorage!(authSessionKey);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "SignPayload",
          value: {
            tag: "Product",
            value: {
              account: PRODUCT_ACCOUNT,
              payload: SIGN_PAYLOAD,
            },
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "SignRaw",
          value: {
            tag: "Product",
            value: {
              account: PRODUCT_ACCOUNT,
              payload: {
                tag: "Bytes",
                value: { bytes: "0x0304" },
              },
            },
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "CreateTransaction",
          value: {
            tag: "Product",
            value: {
              signer: PRODUCT_ACCOUNT,
              genesisHash: GENESIS,
              callData: "0x0506",
              extensions: [],
              txExtVersion: 0,
            },
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "AccountAlias",
          value: {
            callingProductId: "playground.dot",
            context: PROOF_CONTEXT,
            ringLocation: RING_LOCATION,
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "CreateProof",
          value: {
            callingProductId: "playground.dot",
            context: PROOF_CONTEXT,
            ringLocation: RING_LOCATION,
            message: new Uint8Array([7, 8]),
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "AccountAccess",
          value: {
            requestingProductId: "playground.dot",
            targetProductId: "wallet.dot",
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "ResourceAllocation",
          value: {
            resources: [{ tag: "StatementStoreAllowance" }],
          },
        }),
      ),
    ).toBe(true);
    expect(
      await raw.confirmUserAction?.(
        UserConfirmationReview.enc({
          tag: "PreimageSubmit",
          value: { size: 42n },
        }),
      ),
    ).toBe(true);

    await settle();

    expect(preimageEvents).toEqual([null, [4, 5, 6]]);
    expect(calls).toEqual([
      ["lookupPreimage", [9]],
      [
        "authStateChanged",
        { tag: "Pairing", value: { deeplink: "polkadotapp://example" } },
      ],
      ["writeCoreStorage", { tag: "AuthSession", value: undefined }, [3, 2, 1]],
      ["clearCoreStorage", { tag: "AuthSession", value: undefined }],
      ["confirmUserAction:PreimageSubmit", 42n],
    ]);

    disposePreimages?.();
  });

  it("adapts typed result subscriptions", async () => {
    async function* themes() {
      yield ok<ThemeVariant>("Dark");
      yield ok<ThemeVariant>("Light");
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        theme: {
          subscribeTheme: () => themes(),
        },
      }),
    );
    const seen: ThemeVariant[] = [];
    const dispose = raw.subscribeTheme?.((theme) =>
      seen.push(ThemeVariant.dec(theme!)),
    );

    await settle();

    expect(seen).toEqual(["Dark", "Light"]);
    dispose?.();
  });

  it("propagates typed result subscription errors", async () => {
    async function* themes() {
      yield ok<ThemeVariant>("Dark");
      yield err<ThemeVariant, GenericError>({ reason: "theme stream failed" });
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        theme: {
          subscribeTheme: () => themes(),
        },
      }),
    );
    const seen: ThemeVariant[] = [];
    const errors: GenericError[] = [];
    const dispose = raw.subscribeTheme?.(
      (theme) => seen.push(ThemeVariant.dec(theme!)),
      (error) => errors.push(error),
    );

    await settle();

    expect(seen).toEqual(["Dark"]);
    expect(errors).toEqual([{ reason: "theme stream failed" }]);
    dispose?.();
  });

  it("propagates thrown subscription iterator errors", async () => {
    async function* themes() {
      yield ok<ThemeVariant>("Dark");
      throw new Error("theme iterator failed");
    }

    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        theme: {
          subscribeTheme: () => themes(),
        },
      }),
    );
    const seen: ThemeVariant[] = [];
    const errors: GenericError[] = [];
    const dispose = raw.subscribeTheme?.(
      (theme) => seen.push(ThemeVariant.dec(theme!)),
      (error) => errors.push(error),
    );

    await settle();

    expect(seen).toEqual(["Dark"]);
    expect(errors).toEqual([{ reason: "theme iterator failed" }]);
    dispose?.();
  });

  it("bridges typed chain connections", async () => {
    const sent: string[] = [];
    const responses = ['{"jsonrpc":"2.0","id":1,"result":"ok"}'];
    let closes = 0;
    const raw = createWasmRawCallbacks(
      makeHostCallbacks({
        chain: {
          connect: async (genesisHash) => {
            expect([...genesisHash]).toEqual(Array(32).fill(0x11));
            return {
              send(request) {
                sent.push(request);
              },
              async *responses() {
                yield* responses;
              },
              close() {
                closes += 1;
              },
            };
          },
        },
      }),
    );

    expect(typeof raw.chainConnect).toBe("function");
    const received: string[] = [];
    const connection = await raw.chainConnect!(GENESIS, (json) =>
      received.push(json),
    );
    expect(connection).toBeTruthy();

    connection!.send('{"jsonrpc":"2.0","id":1,"method":"system_health"}');
    await settle();

    expect(sent).toEqual(['{"jsonrpc":"2.0","id":1,"method":"system_health"}']);
    expect(received).toEqual(responses);
    connection!.close();
    expect(closes).toBe(1);
  });
});
