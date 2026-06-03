import {
  runExample,
  type LogEntry,
  type RunResult,
} from "./example-runner";
import { getClient } from "./transport";
import type { MethodInfo, ServiceInfo } from "./services";

export const DIAGNOSIS_ID = "__diagnosis__";

export type TestStatus = "idle" | "running" | "pass" | "fail" | "skipped";

export interface TestEntry {
  status: TestStatus;
  request?: string;
  output?: string;
}

const UNARY_TIMEOUT_MS = 10_000;
const SIGNING_TIMEOUT_MS = 30_000;

// Services skipped wholesale in the diagnosis until hosts wire them up.
const SKIPPED_SERVICES = new Set(["Coin Payment"]);
// Methods run last, after the automatic checks: they prompt the user (signing,
// permission/resource requests) or navigate away (`navigate_to`), so deferring
// them keeps each interaction isolated at the end of the run.
const DEFERRED_METHODS = new Set([
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
};

async function runOne({
  serviceName,
  method,
  onUpdate,
}: RunOneOpts): Promise<void> {
  const id = `${serviceName}/${method.name}`;

  if (SKIPPED_SERVICES.has(serviceName)) {
    onUpdate(id, { status: "skipped" });
    return;
  }
  if (!method.exampleSource) {
    onUpdate(id, { status: "fail", output: "no runnable example" });
    return;
  }

  onUpdate(id, { status: "running" });

  const source = method.exampleSource;
  const logs: LogEntry[] = [];
  const onLog = (entry: LogEntry) => logs.push(entry);
  const timeoutMs = LONG_TIMEOUT_METHODS.has(id)
    ? SIGNING_TIMEOUT_MS
    : UNARY_TIMEOUT_MS;

  // The example decides pass/fail explicitly: it resolves on success and throws
  // (via `assert(...)` or any uncaught error) on failure. `console.*` is pure
  // output, captured into `logs` for the report but with no bearing on status.
  let run: RunResult | undefined;
  try {
    run = await runExample({ source, client: getClient(), onLog });
    await Promise.race([
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
      output: joinLogs(logs) ?? "ok",
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const log = joinLogs(logs);
    onUpdate(id, {
      status: "fail",
      request: source,
      output: log ? `${log}\n${message}` : message,
    });
  } finally {
    run?.cancel();
  }
}

function joinLogs(logs: LogEntry[]): string | undefined {
  if (logs.length === 0) return undefined;
  return logs.map((l) => l.text).join("\n");
}

// Re-run a single method, e.g. to replay a failed diagnosis row.
export async function runSingleTest(
  services: ServiceInfo[],
  serviceName: string,
  methodName: string,
  onUpdate: (id: string, entry: TestEntry) => void,
): Promise<void> {
  const svc = services.find((s: ServiceInfo) => s.name === serviceName);
  const method = svc?.methods.find((m: MethodInfo) => m.name === methodName);
  if (!svc || !method) return;
  await runOne({ serviceName, method, onUpdate });
}

// Full diagnosis: run every method one at a time. Automatic checks run first;
// methods that prompt the user or navigate away are deferred to the end so each
// runs in isolation. Produces a complete worked / failed / not-wired matrix
// suitable for the copy-pasteable report.
export async function runDiagnosis(
  services: ServiceInfo[],
  onUpdate: (id: string, entry: TestEntry) => void,
  signal?: AbortSignal,
): Promise<void> {
  const immediate: Array<{ serviceName: string; method: MethodInfo }> = [];
  const deferred: typeof immediate = [];
  for (const svc of services) {
    for (const method of svc.methods) {
      const bucket = DEFERRED_METHODS.has(`${svc.name}/${method.name}`)
        ? deferred
        : immediate;
      bucket.push({ serviceName: svc.name, method });
    }
  }
  for (const { serviceName, method } of [...immediate, ...deferred]) {
    if (signal?.aborted) return;
    await runOne({ serviceName, method, onUpdate });
  }
}
