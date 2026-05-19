import { EMPTY, mergeMap, Observable, of, throwError } from "rxjs";
import type { Client, RemoteChainHeadFollowItem } from "@parity/truapi";

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
    let subscriptionId = "";
    return new Observable<RemoteChainHeadFollowItem>((observer) => {
      const sub = truapi.chain
        .followHeadSubscribe({ request: { genesisHash, withRuntime } })
        .subscribe(observer);
      subscriptionId = sub.subscriptionId;
      return () => {
        sub.unsubscribe();
      };
    }).pipe(
      mergeMap((item) => {
        switch (item.tag) {
          case "Initialized":
            return of<ChainHeadCtx>({
              genesisHash,
              followSubscriptionId: subscriptionId,
              hash: item.value.finalizedBlockHashes[0],
            });
          case "Stop":
            return throwError(() => new Error("follow stopped"));
          case "OperationError":
            return throwError(
              () => new Error(`operation error: ${item.value.error}`),
            );
          case "OperationInaccessible":
            return throwError(() => new Error("operation inaccessible"));
          default:
            return EMPTY;
        }
      }),
    );
  };
}
