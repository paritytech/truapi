import { errorMessage } from "./error.js";
import type { WorkerToMain } from "./worker-protocol.js";

type PostToMain = (msg: WorkerToMain) => void;

function reportDispatchFailure(
  postToMain: PostToMain,
  label: string,
  err: unknown,
): void {
  postToMain({
    kind: "disposeError",
    error: `${label} callback failed: ${errorMessage(err)}`,
  });
}

export function dispatchSubscriptionItem(
  subId: number,
  value: unknown,
  listeners: Map<number, (value: unknown) => void>,
  postToMain: PostToMain,
): void {
  const listener = listeners.get(subId);
  if (!listener) return;
  try {
    listener(value);
  } catch (err) {
    listeners.delete(subId);
    postToMain({ kind: "subscriptionStop", subId });
    reportDispatchFailure(postToMain, `subscription ${subId}`, err);
  }
}

export function dispatchChainResponse(
  connId: number,
  json: string,
  listeners: Map<number, (json: string) => void>,
  postToMain: PostToMain,
): void {
  const listener = listeners.get(connId);
  if (!listener) return;
  try {
    listener(json);
  } catch (err) {
    listeners.delete(connId);
    postToMain({ kind: "chainClose", connId });
    reportDispatchFailure(postToMain, `chain connection ${connId}`, err);
  }
}
