import type { Monaco } from "@monaco-editor/react";
import {
  runExample,
  type LogEntry,
  type RunSubscription,
} from "./example-runner";
import { stringify } from "./host-api-bridge";
import { getClient } from "./transport";
import type { MethodInfo, ServiceInfo } from "./services";

export const AUTO_TEST_ID = "__auto_test__";

export type TestStatus = "idle" | "running" | "pass" | "fail" | "skipped";

export interface TestEntry {
  status: TestStatus;
  request?: string;
  output?: string;
}

export const EXCLUDED_METHODS = new Set([
  "System/navigate_to",
  "System/push_notification",
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
  monaco: Monaco;
  serviceName: string;
  method: MethodInfo;
  onUpdate: (id: string, entry: TestEntry) => void;
  excludeSet: Set<string>;
  signal?: AbortSignal;
  sourceOverride?: string;
};

async function runOne({
  monaco,
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
  if (!method.exampleSource || !method.exampleFunctionName) {
    onUpdate(id, { status: "skipped" });
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
      monaco,
      source,
      functionName: method.exampleFunctionName,
      uri: `file:///playground/auto-test/${serviceName}-${method.name}.ts`,
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
      onUpdate(id, {
        status: "pass",
        request: source,
        output: stringify(value) ?? "null",
      });
    } else {
      await runSubscription(id, source, run.subscription, logs, onUpdate);
    }
  } catch (err) {
    onUpdate(id, {
      status: "fail",
      request: source,
      output: err instanceof Error ? err.message : String(err),
    });
  }
}

async function runSubscription(
  id: string,
  source: string,
  sub: RunSubscription,
  logs: LogEntry[],
  onUpdate: (id: string, entry: TestEntry) => void,
): Promise<void> {
  await new Promise<void>((resolve) => {
    const deadline = setTimeout(() => {
      try {
        sub.unsubscribe();
      } catch {
        /* benign */
      }
      const text = logs.map((l) => l.text).join("\n");
      if (logs.length > 0) {
        onUpdate(id, { status: "pass", request: source, output: text });
      } else {
        onUpdate(id, {
          status: "fail",
          request: source,
          output: `subscription delivered no events in ${SUBSCRIPTION_TIMEOUT_MS / 1000}s`,
        });
      }
      resolve();
    }, SUBSCRIPTION_TIMEOUT_MS);

    const interval = setInterval(() => {
      if (logs.length > 0) {
        clearInterval(interval);
        clearTimeout(deadline);
        try {
          sub.unsubscribe();
        } catch {
          /* benign */
        }
        const text = logs.map((l) => l.text).join("\n");
        onUpdate(id, { status: "pass", request: source, output: text });
        resolve();
      }
    }, 50);
  });
}

export async function runSingleTest(
  monaco: Monaco,
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
    monaco,
    serviceName,
    method,
    onUpdate,
    excludeSet: new Set(),
    sourceOverride,
  });
}

export async function runAutoTests(
  monaco: Monaco,
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
            monaco,
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
            monaco,
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
