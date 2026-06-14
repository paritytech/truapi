import type {
  HostAccountConnectionStatusSubscribeItem,
  Subscription,
} from "@parity/truapi";
import { getClient } from "./transport";

type AccountStatus = HostAccountConnectionStatusSubscribeItem;

type StatusWaiter = {
  status: AccountStatus;
  resolve: (statuses: AccountStatus[]) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

export interface TruapiPlaygroundE2E {
  startAccountConnectionStatusProbe(): AccountStatus[];
  accountConnectionStatuses(): AccountStatus[];
  waitForAccountConnectionStatus(
    status: AccountStatus,
    timeoutMs?: number,
  ): Promise<AccountStatus[]>;
  stopAccountConnectionStatusProbe(): void;
}

declare global {
  interface Window {
    __truapiPlaygroundE2E?: TruapiPlaygroundE2E;
    __TRUAPI_PLAYGROUND_E2E__?: boolean;
  }
}

let accountStatusSub: Subscription | null = null;
let accountStatuses: AccountStatus[] = [];
const waiters = new Set<StatusWaiter>();

function e2eEnabled(): boolean {
  if (window.__TRUAPI_PLAYGROUND_E2E__ === true) return true;
  if (new URLSearchParams(window.location.search).has("e2e")) return true;
  try {
    return window.localStorage.getItem("truapi:playground:e2e") === "1";
  } catch {
    return false;
  }
}

function stopAccountConnectionStatusProbe(): void {
  accountStatusSub?.unsubscribe();
  accountStatusSub = null;
  for (const waiter of waiters) {
    clearTimeout(waiter.timer);
    waiter.reject(new Error("account connection status probe stopped"));
  }
  waiters.clear();
}

function notifyAccountStatus(status: AccountStatus): void {
  accountStatuses.push(status);
  for (const waiter of [...waiters]) {
    if (waiter.status !== status) continue;
    clearTimeout(waiter.timer);
    waiters.delete(waiter);
    waiter.resolve([...accountStatuses]);
  }
}

function startAccountConnectionStatusProbe(): AccountStatus[] {
  stopAccountConnectionStatusProbe();
  accountStatuses = [];
  accountStatusSub = getClient()
    .account.connectionStatusSubscribe()
    .subscribe({
      next: notifyAccountStatus,
      error: (error) => {
        for (const waiter of waiters) {
          clearTimeout(waiter.timer);
          waiter.reject(error);
        }
        waiters.clear();
      },
    });
  return [...accountStatuses];
}

function accountConnectionStatuses(): AccountStatus[] {
  return [...accountStatuses];
}

function waitForAccountConnectionStatus(
  status: AccountStatus,
  timeoutMs = 30_000,
): Promise<AccountStatus[]> {
  if (accountStatuses.includes(status)) {
    return Promise.resolve([...accountStatuses]);
  }
  return new Promise((resolve, reject) => {
    const waiter: StatusWaiter = {
      status,
      resolve,
      reject,
      timer: setTimeout(() => {
        waiters.delete(waiter);
        reject(
          new Error(
            `timed out waiting for account connection status ${status}; saw ${accountStatuses.join(", ")}`,
          ),
        );
      }, timeoutMs),
    };
    waiters.add(waiter);
  });
}

export function installE2EHooks(): void {
  if (!e2eEnabled()) return;
  window.__truapiPlaygroundE2E = {
    startAccountConnectionStatusProbe,
    accountConnectionStatuses,
    waitForAccountConnectionStatus,
    stopAccountConnectionStatusProbe,
  };
}
