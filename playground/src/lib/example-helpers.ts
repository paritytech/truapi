import { Observable } from "rxjs";
import { err, ok, type Result } from "neverthrow";
import { PASEO_NEXT_V2_INDIVIDUALITY } from "@parity/truapi";
import {
  Blake2128Concat,
  Bytes,
  Storage,
} from "@polkadot-api/substrate-bindings";
import type { Client, HexString, StorageResultItem } from "@parity/truapi";

export type ChainHeadCtx = {
  genesisHash: `0x${string}`;
  followSubscriptionId: string;
  hash: `0x${string}`;
};

export type WithChainHeadFollow = (opts: {
  genesisHash: `0x${string}`;
  withRuntime?: boolean;
}) => Observable<ChainHeadCtx>;

const DEFAULT_DOTNS_USERNAME = "pgherveou.05";

export type AccountIdForDotNsUsername = (
  username?: string,
) => Promise<Result<HexString, Error>>;

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
  return function accountIdForDotNsUsername(
    username = DEFAULT_DOTNS_USERNAME,
  ): Promise<Result<HexString, Error>> {
    const key = usernameOwnerOfStorage.enc(
      new TextEncoder().encode(username),
    ) as HexString;

    return new Promise<Result<HexString, Error>>((resolve) => {
      let operationId: string | null = null;
      const sub = truapi.chain
        .followHeadSubscribe({
          request: {
            genesisHash: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
            withRuntime: false,
          },
        })
        .subscribe({
          next: async (item) => {
            const fail = (reason: unknown) => {
              sub.unsubscribe();
              resolve(err(toError(reason)));
            };

            try {
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
                      fail(`No account owns DotNS username "${username}"`);
                      return;
                    }
                    sub.unsubscribe();
                    resolve(ok(account));
                  }
                  return;
                case "OperationStorageDone":
                  if (item.value.operationId === operationId) {
                    fail(`No account owns DotNS username "${username}"`);
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
            } catch (error) {
              sub.unsubscribe();
              resolve(err(toError(error)));
            }
          },
          error: (error) => resolve(err(toError(error))),
          complete: () =>
            resolve(
              err(
                new Error(
                  "chain head subscription completed before username lookup finished",
                ),
              ),
            ),
        });
    });
  };
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
  return value instanceof Error ? value : new Error(String(value));
}
