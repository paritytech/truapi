import {
  runExample,
  type LogEntry,
  type RunSubscription,
} from "./example-runner";
import { stringify } from "./host-api-bridge";
import { errorTextFrom } from "./result-status";
import { getClient } from "./transport";
import type { MethodInfo, ServiceInfo } from "./services";

export const AUTO_TEST_ID = "__auto_test__";
export const DIAGNOSIS_ID = "__diagnosis__";

export type TestStatus = "idle" | "running" | "pass" | "fail" | "skipped";

export interface TestEntry {
  status: TestStatus;
  request?: string;
  output?: string;
}

export const EXCLUDED_METHODS = new Set([
  "System/navigate_to",
  "Permissions/request_device_permission",
  "Permissions/request_remote_permission",
  "Resource Allocation/request",
  "Signing/sign_payload",
  "Signing/sign_raw",
  "Signing/sign_raw_with_legacy_account",
  "Signing/sign_payload_with_legacy_account",
  "Signing/create_transaction",
  "Signing/create_transaction_with_legacy_account",
  "Account/get_account_alias",
]);

const UNARY_TIMEOUT_MS = 4_000;
const SIGNING_TIMEOUT_MS = 30_000;
const SUBSCRIPTION_TIMEOUT_MS = 6_000;

const CONCURRENCY = 6;
// Chain examples open ephemeral follow subscriptions inline. Running the
// service serially avoids spawning many concurrent follow streams.
const SERIAL_SERVICES = new Set(["Chain"]);
const LONG_TIMEOUT_METHODS = new Set([
  "Resource Allocation/request",
  "Signing/sign_payload",
  "Signing/sign_raw",
  "Signing/sign_raw_with_legacy_account",
  "Signing/sign_payload_with_legacy_account",
  "Signing/create_transaction",
  "Signing/create_transaction_with_legacy_account",
]);

type RunOneOpts = {
  serviceName: string;
  method: MethodInfo;
  onUpdate: (id: string, entry: TestEntry) => void;
  excludeSet: Set<string>;
  signal?: AbortSignal;
  sourceOverride?: string;
};

async function runOne({
  serviceName,
  method,
  onUpdate,
  excludeSet,
  signal,
  sourceOverride,
}: RunOneOpts): Promise<void> {
  if (signal?.aborted) return;
  const id = `${serviceName}/${method.name}`;

  if (excludeSet.has(id)) {
    onUpdate(id, { status: "skipped" });
    return;
  }
  if (!method.exampleSource) {
    onUpdate(id, { status: "fail", output: "no runnable example" });
    return;
  }

  onUpdate(id, { status: "running" });

  const source = sourceOverride ?? method.exampleSource;
  const logs: LogEntry[] = [];
  const onLog = (entry: LogEntry) => logs.push(entry);
  const timeoutMs = LONG_TIMEOUT_METHODS.has(id)
    ? SIGNING_TIMEOUT_MS
    : UNARY_TIMEOUT_MS;

  try {
    const run = await runExample({
      source,
      kind: method.type,
      client: getClient(),
      onLog,
    });

    if (run.kind === "unary") {
      const value = await Promise.race([
        run.promise,
        new Promise<never>((_, reject) =>
          setTimeout(
            () => reject(new Error(`timed out after ${timeoutMs / 1000}s`)),
            timeoutMs,
          ),
        ),
      ]);
      // Generated examples self-handle the Result via
      // `result.match(v => console.log(v), e => console.error(e))`, so an Err
      // surfaces as an error-level log rather than a thrown exception. Some
      // examples instead `return result` / `console.log(result)`, leaving the
      // neverthrow Err as the resolved value. Treat either as an errored call.
      const errText = errorTextFrom(value, logs);
      if (errText != null) {
        onUpdate(id, {
          status: "fail",
          request: source,
          output: errText,
        });
      } else {
        onUpdate(id, {
          status: "pass",
          request: source,
          output: stringify(value) ?? joinLogs(logs) ?? "null",
        });
      }
    } else {
      await runSubscription(id, source, run.subscription, logs, onUpdate);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    onUpdate(id, {
      status: "fail",
      request: source,
      output: message,
    });
  }
}

function joinLogs(logs: LogEntry[]): string | undefined {
  if (logs.length === 0) return undefined;
  return logs.map((l) => l.text).join("\n");
}

async function runSubscription(
  id: string,
  source: string,
  sub: RunSubscription,
  logs: LogEntry[],
  onUpdate: (id: string, entry: TestEntry) => void,
): Promise<void> {
  const settle = (resolve: () => void) => {
    try {
      sub.unsubscribe();
    } catch {
      /* benign */
    }
    const errText = errorTextFrom(undefined, logs);
    if (errText != null) {
      onUpdate(id, {
        status: "fail",
        request: source,
        output: errText,
      });
    } else if (logs.length > 0) {
      onUpdate(id, {
        status: "pass",
        request: source,
        output: logs.map((l) => l.text).join("\n"),
      });
    } else {
      onUpdate(id, {
        status: "fail",
        request: source,
        output: `subscription delivered no events in ${SUBSCRIPTION_TIMEOUT_MS / 1000}s`,
      });
    }
    resolve();
  };

  await new Promise<void>((resolve) => {
    const interval = setInterval(() => {
      if (logs.length > 0) {
        clearInterval(interval);
        clearTimeout(deadline);
        settle(resolve);
      }
    }, 50);
    const deadline = setTimeout(() => {
      clearInterval(interval);
      settle(resolve);
    }, SUBSCRIPTION_TIMEOUT_MS);
  });
}

export async function runSingleTest(
  services: ServiceInfo[],
  serviceName: string,
  methodName: string,
  onUpdate: (id: string, entry: TestEntry) => void,
  sourceOverride?: string,
): Promise<void> {
  const svc = services.find((s: ServiceInfo) => s.name === serviceName);
  const method = svc?.methods.find((m: MethodInfo) => m.name === methodName);
  if (!svc || !method) return;
  await runOne({
    serviceName,
    method,
    onUpdate,
    excludeSet: new Set(),
    sourceOverride,
  });
}

export async function runAutoTests(
  services: ServiceInfo[],
  onUpdate: (id: string, entry: TestEntry) => void,
  signal?: AbortSignal,
  excludeSet: Set<string> = EXCLUDED_METHODS,
): Promise<void> {
  const tasks: Array<() => Promise<void>> = [];
  for (const svc of services) {
    if (SERIAL_SERVICES.has(svc.name)) {
      tasks.push(async () => {
        for (const m of svc.methods) {
          if (signal?.aborted) return;
          await runOne({
            serviceName: svc.name,
            method: m,
            onUpdate,
            excludeSet,
            signal,
          });
        }
      });
    } else {
      for (const m of svc.methods) {
        tasks.push(() =>
          runOne({
            serviceName: svc.name,
            method: m,
            onUpdate,
            excludeSet,
            signal,
          }),
        );
      }
    }
  }

  let cursor = 0;
  const workerCount = Math.min(CONCURRENCY, tasks.length);
  await Promise.all(
    Array.from({ length: workerCount }, async () => {
      while (cursor < tasks.length && !signal?.aborted) {
        const task = tasks[cursor++];
        await task();
      }
    }),
  );
}

// Full diagnosis: run every non-disruptive method in parallel, then run each
// disruptive method (signing, permission/resource requests, `navigate_to`)
// sequentially — one at a time — so the human can complete each phone
// interaction before the next begins. Produces a complete worked / failed /
// not-wired matrix suitable for the copy-pasteable report.
export async function runDiagnosis(
  services: ServiceInfo[],
  onUpdate: (id: string, entry: TestEntry) => void,
  signal?: AbortSignal,
): Promise<void> {
  await runAutoTests(services, onUpdate, signal, EXCLUDED_METHODS);

  const byId = new Map<string, { serviceName: string; method: MethodInfo }>();
  for (const svc of services) {
    for (const m of svc.methods) {
      byId.set(`${svc.name}/${m.name}`, { serviceName: svc.name, method: m });
    }
  }

  for (const id of EXCLUDED_METHODS) {
    if (signal?.aborted) return;
    const entry = byId.get(id);
    if (!entry) continue;
    await runOne({
      serviceName: entry.serviceName,
      method: entry.method,
      onUpdate,
      excludeSet: new Set(),
      signal,
    });
  }
}
