import { runExample, type LogEntry, type RunResult } from "./example-runner";
import { getClientSync } from "@parity/truapi/sandbox";
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
const SSO_TIMEOUT_MS = 60_000;

// Services skipped wholesale in the diagnosis until hosts wire them up.
const SKIPPED_SERVICES = new Set(["Coin Payment"]);
// Methods whose first call implicitly triggers a host permission/signing
// prompt, so they need the longer signing-class timeout to allow for the user
// to respond. `get_account_alias` and `Preimage/submit` prompt on first use.
const LONG_TIMEOUT_METHODS = new Set([
  "Account/get_account_alias",
  "Resource Allocation/request",
  "Signing/sign_payload",
  "Signing/sign_raw",
  "Signing/sign_raw_with_legacy_account",
  "Signing/sign_payload_with_legacy_account",
  "Signing/create_transaction",
  "Signing/create_transaction_with_legacy_account",
  "Preimage/submit",
]);

const METHOD_TIMEOUT_MS = new Map<string, number>([
  ["Account/get_account_alias", SSO_TIMEOUT_MS],
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
  const timeoutMs =
    METHOD_TIMEOUT_MS.get(id) ??
    (LONG_TIMEOUT_METHODS.has(id) ? SIGNING_TIMEOUT_MS : UNARY_TIMEOUT_MS);
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timeout = setTimeout(
      () => reject(new Error(`timed out after ${timeoutMs / 1000}s`)),
      timeoutMs,
    );
  });

  // The example decides pass/fail explicitly: it resolves on success and throws
  // (via `assert(...)` or any uncaught error) on failure. `console.*` is pure
  // output, captured into `logs` for the report but with no bearing on status.
  let run: RunResult | undefined;
  try {
    const client = getClientSync();
    if (!client) {
      throw new Error(
        "App must be opened inside a TrUAPI host (iframe or webview).",
      );
    }
    run = await Promise.race([
      runExample({ source, client, onLog }),
      timeoutPromise,
    ]);
    await Promise.race([run.promise, timeoutPromise]);
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
    if (timeout !== undefined) clearTimeout(timeout);
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

// Full diagnosis: run every method one at a time, in service order. Methods
// that prompt the user (signing, permission/resource requests) block on their
// host dialog before the run continues. Produces a complete worked / failed /
// not-wired matrix suitable for the copy-pasteable report.
export async function runDiagnosis(
  services: ServiceInfo[],
  onUpdate: (id: string, entry: TestEntry) => void,
  signal?: AbortSignal,
): Promise<void> {
  for (const svc of services) {
    for (const method of svc.methods) {
      if (signal?.aborted) return;
      await runOne({ serviceName: svc.name, method, onUpdate });
    }
  }
}
