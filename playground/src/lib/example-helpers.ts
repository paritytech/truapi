import { Observable } from "rxjs";
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

export type AccountIdForDotNsUsername = (username: string) => Promise<HexString>;

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
  return function accountIdForDotNsUsername(username: string): Promise<HexString> {
    const key = usernameOwnerOfStorage.enc(
      new TextEncoder().encode(username),
    ) as HexString;

    return new Promise<HexString>((resolve, reject) => {
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
                    throw result.error;
                  }
                  if (result.value.operation.tag !== "Started") {
                    throw new Error("getHeadStorage operation limit reached");
                  }
                  operationId = result.value.operation.value.operationId;
                  return;
                }
                case "OperationStorageItems":
                  if (item.value.operationId === operationId) {
                    const account = findStorageValue(item.value.items, key);
                    if (!account) {
                      throw new Error(
                        `No account owns DotNS username "${username}"`,
                      );
                    }
                    sub.unsubscribe();
                    resolve(account);
                  }
                  return;
                case "OperationStorageDone":
                  if (item.value.operationId === operationId) {
                    throw new Error(
                      `No account owns DotNS username "${username}"`,
                    );
                  }
                  return;
                case "OperationError":
                  if (item.value.operationId === operationId) {
                    throw new Error(`getHeadStorage failed: ${item.value.error}`);
                  }
                  return;
                case "OperationInaccessible":
                  if (item.value.operationId === operationId) {
                    throw new Error("getHeadStorage operation inaccessible");
                  }
                  return;
                case "Stop":
                  throw new Error(
                    "chain head subscription stopped before username lookup finished",
                  );
              }
            } catch (err) {
              sub.unsubscribe();
              reject(err);
            }
          },
          error: reject,
          complete: () =>
            reject(
              new Error(
                "chain head subscription completed before username lookup finished",
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
