import { Observable } from "rxjs";
import type { Client } from "@parity/truapi";

export type ChainHeadCtx = {
  genesisHash: `0x${string}`;
  followSubscriptionId: string;
  hash: `0x${string}`;
};

export type WithChainHeadFollow = (opts: {
  genesisHash: `0x${string}`;
  withRuntime?: boolean;
}) => Observable<ChainHeadCtx>;

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
                observer.error(new Error("follow stopped"));
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
