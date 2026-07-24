import { Observable } from "rxjs";
import { err, ok, type Result } from "neverthrow";
import { PASEO_NEXT_V2_INDIVIDUALITY } from "@parity/truapi";
import {
  AccountId,
  Blake2128Concat,
  Bytes,
  decAnyMetadata,
  Storage,
  unifyMetadata,
} from "@polkadot-api/substrate-bindings";
import {
  getDynamicBuilder,
  getLookupFn,
} from "@polkadot-api/metadata-builders";
import { fromHex, toHex } from "@polkadot-api/utils";
import type {
  Client,
  HexString,
  ProductAccountId,
  ProductAccountTxPayload,
  RemoteChainHeadFollowItem,
  RuntimeSpec,
  RuntimeType,
  StorageResultItem,
  TxPayloadExtension,
} from "@parity/truapi";

export type ChainHeadCtx = {
  genesisHash: `0x${string}`;
  followSubscriptionId: string;
  hash: `0x${string}`;
};

export type WithChainHeadFollow = (opts: {
  genesisHash: `0x${string}`;
  withRuntime?: boolean;
}) => Observable<ChainHeadCtx>;

export type AccountIdForDotNsUsername = (
  username?: string,
) => Promise<Result<HexString, Error>>;

export type Ss58AddressForDotNsUsername = (
  username?: string,
) => Promise<Result<string, Error>>;

export type BuildCreateTransactionPayload = (opts: {
  signer: ProductAccountId;
  genesisHash: HexString;
  callData: HexString;
}) => Promise<Result<ProductAccountTxPayload, Error>>;

const usernameOwnerOfStorage = Storage("Resources")("UsernameOwnerOf", [
  Bytes(),
  Blake2128Concat,
]);

export function createWithChainHeadFollow(truapi: Client): WithChainHeadFollow {
  return function withChainHeadFollow({
    genesisHash,
    withRuntime = false,
  }): Observable<ChainHeadCtx> {
    return new Observable<ChainHeadCtx>((observer) => {
      const sub = truapi.chain
        .followHeadSubscribe({ request: { genesisHash, withRuntime } })
        .subscribe({
          next: (item) => {
            switch (item.tag) {
              case "Initialized":
                observer.next({
                  genesisHash,
                  followSubscriptionId: sub.subscriptionId,
                  hash: item.value.finalizedBlockHashes[0],
                });
                return;
              case "Stop":
                observer.complete();
                return;
              case "OperationError":
                observer.error(
                  new Error(`operation error: ${item.value.error}`),
                );
                return;
              case "OperationInaccessible":
                observer.error(new Error("operation inaccessible"));
                return;
            }
          },
          error: (err) => observer.error(err),
          complete: () => observer.complete(),
        });
      return () => sub.unsubscribe();
    });
  };
}

export function createAccountIdForDotNsUsername(
  truapi: Client,
): AccountIdForDotNsUsername {
  return async function accountIdForDotNsUsername(
    username,
  ): Promise<Result<HexString, Error>> {
    let dotNsUsername = username;
    if (dotNsUsername === undefined) {
      const userIdResult = await truapi.account.getUserId();
      if (userIdResult.isErr()) {
        return err(toError(userIdResult.error));
      }
      dotNsUsername = userIdResult.value.primaryUsername;
    }
    if (dotNsUsername.length === 0) {
      return err(new Error("DotNS username is empty"));
    }

    const key = usernameOwnerOfStorage.enc(
      new TextEncoder().encode(dotNsUsername),
    ) as HexString;

    return new Promise<Result<HexString, Error>>((resolve) => {
      let operationId: string | null = null;
      let settled = false;
      let eventQueue = Promise.resolve();
      const fail = (reason: unknown) => {
        if (settled) return;
        settled = true;
        sub.unsubscribe();
        resolve(err(toError(reason)));
      };
      const succeed = (account: HexString) => {
        if (settled) return;
        settled = true;
        sub.unsubscribe();
        resolve(ok(account));
      };
      const handleItem = async (item: RemoteChainHeadFollowItem) => {
        switch (item.tag) {
          case "Initialized": {
            const result = await truapi.chain.getHeadStorage({
              genesisHash: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
              followSubscriptionId: sub.subscriptionId,
              hash: item.value.finalizedBlockHashes[0],
              items: [{ key, queryType: "Value" }],
            });
            if (result.isErr()) {
              fail(result.error);
              return;
            }
            if (result.value.operation.tag !== "Started") {
              fail(new Error("getHeadStorage operation limit reached"));
              return;
            }
            operationId = result.value.operation.value.operationId;
            return;
          }
          case "OperationStorageItems":
            if (item.value.operationId === operationId) {
              const account = findStorageValue(item.value.items, key);
              if (!account) {
                fail(`No account owns DotNS username "${dotNsUsername}"`);
                return;
              }
              succeed(account);
            }
            return;
          case "OperationStorageDone":
            if (item.value.operationId === operationId) {
              fail(`No account owns DotNS username "${dotNsUsername}"`);
            }
            return;
          case "OperationError":
            if (item.value.operationId === operationId) {
              fail(`getHeadStorage failed: ${item.value.error}`);
            }
            return;
          case "OperationInaccessible":
            if (item.value.operationId === operationId) {
              fail("getHeadStorage operation inaccessible");
            }
            return;
          case "Stop":
            fail(
              "chain head subscription stopped before username lookup finished",
            );
            return;
        }
      };
      const sub = truapi.chain
        .followHeadSubscribe({
          request: {
            genesisHash: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
            withRuntime: false,
          },
        })
        .subscribe({
          next: (item) => {
            // RxJS does not await async `next` handlers. Serialize follow
            // events so storage items cannot overtake the unary response that
            // tells us which operation ID to match.
            eventQueue = eventQueue
              .then(() => handleItem(item))
              .catch((error) => fail(error));
          },
          error: (error) => fail(error),
          complete: () => {
            eventQueue = eventQueue.then(() =>
              fail(
                new Error(
                  "chain head subscription completed before username lookup finished",
                ),
              ),
            );
          },
        });
    });
  };
}

export function createSs58AddressForDotNsUsername(
  accountIdForDotNsUsername: AccountIdForDotNsUsername,
): Ss58AddressForDotNsUsername {
  return async function ss58AddressForDotNsUsername(username) {
    const accountId = await accountIdForDotNsUsername(username);
    if (accountId.isErr()) return err(accountId.error);

    try {
      return ok(AccountId().dec(fromHex(accountId.value)));
    } catch (error) {
      return err(toError(error));
    }
  };
}

export function createBuildCreateTransactionPayload(
  truapi: Client,
): BuildCreateTransactionPayload {
  return async function buildCreateTransactionPayload(opts) {
    const accountResult = await truapi.account.getAccount({
      productAccountId: opts.signer,
    });
    if (accountResult.isErr()) {
      return err(toError(accountResult.error));
    }

    const built = await buildTransactionContext(
      truapi,
      opts.genesisHash,
      accountResult.value.account.publicKey,
    );
    if (built.isErr()) return err(built.error);

    const { metadata, runtime, nonce, genesisHash } = built.value;
    const unified = unifyMetadata(decAnyMetadata(metadata));
    const lookupFn = getLookupFn(unified);
    const builder = getDynamicBuilder(lookupFn);
    const chainState = {
      genesisHash: fromHex(genesisHash),
      specVersion: runtime.specVersion,
      transactionVersion: runtime.transactionVersion ?? 0,
      nonce,
    };

    return ok({
      signer: opts.signer,
      genesisHash,
      callData: opts.callData,
      extensions: encodeSignedExtensions(
        unified,
        lookupFn,
        builder,
        chainState,
      ),
      txExtVersion: txExtVersionFromMetadata(unified),
    });
  };
}

type UnifiedMetadata = ReturnType<typeof unifyMetadata>;
type LookupFn = ReturnType<typeof getLookupFn>;
type LookupEntry = ReturnType<LookupFn>;
type DynamicBuilder = ReturnType<typeof getDynamicBuilder>;

type ChainState = {
  genesisHash: Uint8Array;
  specVersion: number;
  transactionVersion: number;
  nonce: number;
};

type TransactionContext = {
  genesisHash: HexString;
  metadata: Uint8Array;
  nonce: number;
  runtime: RuntimeSpec;
};

function buildTransactionContext(
  truapi: Client,
  genesisHash: HexString,
  accountPublicKey: HexString,
): Promise<Result<TransactionContext, Error>> {
  return new Promise((resolve) => {
    let subscription: ReturnType<
      ReturnType<Client["chain"]["followHeadSubscribe"]>["subscribe"]
    > | null = null;
    const completedOperations = new Map<string, Result<HexString, Error>>();
    const operationWaiters = new Map<
      string,
      (result: Result<HexString, Error>) => void
    >();
    let initialized = false;
    let settled = false;

    const settle = (result: Result<TransactionContext, Error>) => {
      if (settled) return;
      settled = true;
      try {
        subscription?.unsubscribe();
      } catch {
        /* benign */
      }
      resolve(result);
    };

    const finishOperation = (
      operationId: string,
      result: Result<HexString, Error>,
    ) => {
      const waiter = operationWaiters.get(operationId);
      if (waiter) {
        operationWaiters.delete(operationId);
        waiter(result);
        return;
      }
      completedOperations.set(operationId, result);
    };

    const waitForOperation = (
      operationId: string,
    ): Promise<Result<HexString, Error>> => {
      const completed = completedOperations.get(operationId);
      if (completed) {
        completedOperations.delete(operationId);
        return Promise.resolve(completed);
      }
      return new Promise((operationResolve) => {
        operationWaiters.set(operationId, operationResolve);
      });
    };

    const callHead = async (
      hash: HexString,
      fn: string,
      callParameters: HexString,
    ): Promise<Result<HexString, Error>> => {
      if (!subscription) {
        return err(new Error("chain head subscription was not initialized"));
      }
      const result = await truapi.chain.callHead({
        genesisHash,
        followSubscriptionId: subscription.subscriptionId,
        hash,
        function: fn,
        callParameters,
      });
      if (result.isErr()) return err(toError(result.error));
      if (result.value.operation.tag !== "Started") {
        return err(new Error(`chainHead call limit reached for ${fn}`));
      }
      return waitForOperation(result.value.operation.value.operationId);
    };

    const handleInitialized = async (
      item: Extract<RemoteChainHeadFollowItem, { tag: "Initialized" }>,
    ) => {
      if (initialized) return;
      initialized = true;
      const hash = item.value.finalizedBlockHashes[0];
      if (!hash) {
        settle(
          err(new Error("chainHead initialized without a finalized hash")),
        );
        return;
      }
      const runtime = runtimeSpecFrom(item.value.finalizedBlockRuntime);
      if (runtime.isErr()) {
        settle(err(runtime.error));
        return;
      }

      const [metadata, nonce] = await Promise.all([
        callHead(hash, "Metadata_metadata", "0x"),
        callHead(hash, "AccountNonceApi_account_nonce", accountPublicKey),
      ]);
      if (metadata.isErr()) {
        settle(err(metadata.error));
        return;
      }
      if (nonce.isErr()) {
        settle(err(nonce.error));
        return;
      }

      const rawMetadata = unwrapOpaqueMetadata(metadata.value);
      if (rawMetadata.isErr()) {
        settle(err(rawMetadata.error));
        return;
      }

      let decodedNonce: number;
      try {
        decodedNonce = nonceFromRuntimeApiOutput(nonce.value);
      } catch (error) {
        settle(err(toError(error)));
        return;
      }

      const followSubscriptionId = subscription?.subscriptionId;
      if (followSubscriptionId) {
        void truapi.chain.unpinHead({
          genesisHash,
          followSubscriptionId,
          hashes: [hash],
        });
      }

      settle(
        ok({
          genesisHash,
          metadata: rawMetadata.value,
          nonce: decodedNonce,
          runtime: runtime.value,
        }),
      );
    };

    subscription = truapi.chain
      .followHeadSubscribe({
        request: { genesisHash, withRuntime: true },
      })
      .subscribe({
        next: (item) => {
          switch (item.tag) {
            case "Initialized":
              void handleInitialized(item);
              return;
            case "OperationCallDone":
              finishOperation(item.value.operationId, ok(item.value.output));
              return;
            case "OperationError":
              finishOperation(
                item.value.operationId,
                err(
                  new Error(`chainHead operation failed: ${item.value.error}`),
                ),
              );
              return;
            case "OperationInaccessible":
              finishOperation(
                item.value.operationId,
                err(new Error("chainHead operation inaccessible")),
              );
              return;
            case "Stop":
              settle(
                err(
                  new Error(
                    "chain head subscription stopped before transaction context was built",
                  ),
                ),
              );
              return;
          }
        },
        error: (error) => settle(err(toError(error))),
        complete: () =>
          settle(
            err(
              new Error(
                "chain head subscription completed before transaction context was built",
              ),
            ),
          ),
      });
  });
}

function runtimeSpecFrom(value?: RuntimeType): Result<RuntimeSpec, Error> {
  if (!value) return err(new Error("chainHead did not include runtime data"));
  if (value.tag === "Invalid") {
    return err(new Error(`chainHead runtime invalid: ${value.value.error}`));
  }
  if (value.value.transactionVersion === undefined) {
    return err(new Error("runtime did not include transactionVersion"));
  }
  return ok(value.value);
}

function unwrapOpaqueMetadata(output: HexString): Result<Uint8Array, Error> {
  try {
    const raw = Bytes().dec(fromHex(output));
    if (
      raw.length < 5 ||
      raw[0] !== 0x6d ||
      raw[1] !== 0x65 ||
      raw[2] !== 0x74 ||
      raw[3] !== 0x61
    ) {
      return err(
        new Error("runtime Metadata_metadata returned invalid metadata"),
      );
    }
    return ok(raw);
  } catch (error) {
    return err(toError(error));
  }
}

function nonceFromRuntimeApiOutput(output: HexString): number {
  const bytes = fromHex(output);
  if (bytes.length < 4) {
    throw new Error("AccountNonceApi_account_nonce returned too few bytes");
  }
  return new DataView(
    bytes.buffer,
    bytes.byteOffset,
    bytes.byteLength,
  ).getUint32(0, true);
}

function txExtVersionFromMetadata(metadata: UnifiedMetadata): number {
  const latestVersion = metadata.extrinsic.version.reduce(
    (max, version) => Math.max(max, version),
    0,
  );
  return latestVersion === 4 ? 0 : latestVersion;
}

function encodeSignedExtensions(
  metadata: UnifiedMetadata,
  lookupFn: LookupFn,
  builder: DynamicBuilder,
  chainState: ChainState,
): TxPayloadExtension[] {
  const exts = metadata.extrinsic.signedExtensions[0] as Array<{
    identifier: string;
    type: number;
    additionalSigned: number;
  }>;

  return exts.map((ext) => {
    const values = signedExtensionValues(ext, lookupFn, chainState);
    const extra = encodeExtensionField(
      builder,
      lookupFn,
      ext.type,
      values.extra,
    );
    const additionalSigned = encodeExtensionField(
      builder,
      lookupFn,
      ext.additionalSigned,
      values.additionalSigned,
    );

    return {
      id: ext.identifier,
      extra: toHex(extra) as HexString,
      additionalSigned: toHex(additionalSigned) as HexString,
    };
  });
}

function signedExtensionValues(
  ext: { identifier: string; type: number; additionalSigned: number },
  lookupFn: LookupFn,
  chainState: ChainState,
): { extra: unknown; additionalSigned: unknown } {
  switch (ext.identifier) {
    case "CheckNonce":
      return { extra: chainState.nonce, additionalSigned: undefined };
    case "CheckSpecVersion":
      return {
        extra: undefined,
        additionalSigned: chainState.specVersion,
      };
    case "CheckTxVersion":
      return {
        extra: undefined,
        additionalSigned: chainState.transactionVersion,
      };
    case "CheckGenesis":
      return {
        extra: undefined,
        additionalSigned: toHex(chainState.genesisHash),
      };
    case "CheckMortality":
      return {
        extra: { type: "Immortal" },
        additionalSigned: toHex(chainState.genesisHash),
      };
    case "VerifyMultiSignature":
      return { extra: { type: "Disabled" }, additionalSigned: undefined };
    case "ChargeAssetTxPayment":
      return {
        extra: { tip: 0, asset_id: undefined },
        additionalSigned: undefined,
      };
    case "RestrictOrigins":
      return { extra: false, additionalSigned: undefined };
    default:
      return {
        extra: defaultValueForType(lookupFn(ext.type)),
        additionalSigned: defaultValueForType(lookupFn(ext.additionalSigned)),
      };
  }
}

function encodeExtensionField(
  builder: DynamicBuilder,
  lookupFn: LookupFn,
  typeId: number,
  value: unknown,
): Uint8Array {
  const entry = lookupFn(typeId);
  if (!entry || entry.type === "void") return new Uint8Array(0);
  const codec = builder.buildDefinition(typeId) as {
    enc: (value: unknown) => Uint8Array;
  };
  return codec.enc(value);
}

function defaultValueForType(entry: LookupEntry): unknown {
  if (!entry) return undefined;
  if (entry.type === "void" || entry.type === "option") return undefined;
  if (entry.type === "primitive") {
    if (entry.value === "bool") return false;
    if (entry.value.startsWith("u") || entry.value.startsWith("i")) return 0;
    return undefined;
  }
  if (entry.type === "compact") return 0;
  if (entry.type === "array") return new Uint8Array(entry.len);
  if (entry.type === "enum") {
    const first = Object.entries(entry.value)[0];
    if (!first) return undefined;
    const [name, variant] = first;
    if (variant.type === "void") return { type: name };
    return { type: name, value: undefined };
  }
  return undefined;
}

function findStorageValue(
  items: StorageResultItem[],
  key: HexString,
): HexString | null {
  const item = items.find(
    (candidate) => candidate.key.toLowerCase() === key.toLowerCase(),
  );
  return item?.value ?? null;
}

function toError(value: unknown): Error {
  if (value instanceof Error) return value;
  if (typeof value === "string") return new Error(value);
  try {
    return new Error(JSON.stringify(value));
  } catch {
    return new Error(String(value));
  }
}
