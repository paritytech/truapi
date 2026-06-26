import type {
  HostAccountConnectionStatusSubscribeItem,
  Subscription,
} from "@parity/truapi";
import {
  getClientSync,
  subscribeConnectionStatus,
  type ConnectionStatus,
} from "@parity/truapi/sandbox";

type AccountStatus = HostAccountConnectionStatusSubscribeItem;

type StatusWaiter = {
  status: AccountStatus;
  resolve: (statuses: AccountStatus[]) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

export interface TruapiPlaygroundE2E {
  connectionStatus(): ConnectionStatus;
  waitForConnectionStatus(
    status: ConnectionStatus,
    timeoutMs?: number,
  ): Promise<ConnectionStatus>;
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
let hostStatus: ConnectionStatus = "connecting";
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
  const client = getClientSync();
  if (!client) {
    throw new Error("App must be opened inside a TrUAPI host.");
  }
  accountStatusSub = client.account.connectionStatusSubscribe().subscribe({
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

function connectionStatus(): ConnectionStatus {
  return hostStatus;
}

function waitForConnectionStatus(
  status: ConnectionStatus,
  timeoutMs = 30_000,
): Promise<ConnectionStatus> {
  if (hostStatus === status) {
    return Promise.resolve(hostStatus);
  }

  return new Promise((resolve, reject) => {
    let done = false;
    let unsubscribe: (() => void) | null = null;
    let unsubscribeAfterSubscribe = false;
    const finish = (
      next: ConnectionStatus,
      error?: Error,
    ): void => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      if (unsubscribe) {
        unsubscribe();
      } else {
        unsubscribeAfterSubscribe = true;
      }
      if (error) {
        reject(error);
      } else {
        resolve(next);
      }
    };
    const timer = setTimeout(() => {
      finish(
        hostStatus,
        new Error(
          `timed out waiting for host connection status ${status}; current status is ${hostStatus}`,
        ),
      );
    }, timeoutMs);

    try {
      unsubscribe = subscribeConnectionStatus((next) => {
        hostStatus = next;
        if (next === status) {
          finish(next);
        }
      });
      if (unsubscribeAfterSubscribe) {
        unsubscribe();
      }
    } catch (error) {
      finish(
        hostStatus,
        error instanceof Error ? error : new Error(String(error)),
      );
    }
  });
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
    connectionStatus,
    waitForConnectionStatus,
    startAccountConnectionStatusProbe,
    accountConnectionStatuses,
    waitForAccountConnectionStatus,
    stopAccountConnectionStatusProbe,
  };
}
