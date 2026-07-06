// Runs the playground's own generated example sources against a headless
// pairing host, using the playground's `runExample` so these are literally the
// tests the playground diagnosis runs. Pass/fail is decided by the example
// body (it resolves on success, throws via `assert` on failure), exactly as in
// `playground/src/lib/auto-test.ts`.
import {
  runExample,
  type LogEntry,
} from "../../../../playground/src/lib/example-runner.ts";
import { services } from "../../../../js/packages/truapi/src/playground/codegen/services.ts";
import type { TrUApiClient } from "../../../../js/packages/truapi/src/index.ts";

// Mirrors auto-test.ts.
const UNARY_TIMEOUT_MS = 10_000;
const SIGNING_TIMEOUT_MS = 30_000;
const SSO_TIMEOUT_MS = 60_000;
const SKIPPED_SERVICES = new Set(["Chat", "Coin Payment", "Payment"]);
const SKIPPED_METHODS = new Set(["Account/create_account_proof"]);
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
  ["Resource Allocation/request", SSO_TIMEOUT_MS],
  ["Preimage/lookup_subscribe", SSO_TIMEOUT_MS],
  ["Preimage/submit", SSO_TIMEOUT_MS],
  ["Signing/create_transaction", SSO_TIMEOUT_MS],
]);

export type DiagnosisStatus = "pass" | "fail" | "skipped";
export interface DiagnosisRow {
  id: string;
  status: DiagnosisStatus;
  output: string;
}

async function runOne(
  client: TrUApiClient,
  serviceName: string,
  method: { name: string; exampleSource?: string },
): Promise<DiagnosisRow> {
  const id = `${serviceName}/${method.name}`;
  if (SKIPPED_SERVICES.has(serviceName) || SKIPPED_METHODS.has(id)) {
    return { id, status: "skipped", output: "" };
  }
  if (!method.exampleSource) {
    return { id, status: "fail", output: "no runnable example" };
  }
  const timeoutMs =
    METHOD_TIMEOUT_MS.get(id) ??
    (LONG_TIMEOUT_METHODS.has(id) ? SIGNING_TIMEOUT_MS : UNARY_TIMEOUT_MS);

  const logs: LogEntry[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out after ${timeoutMs / 1000}s`)), timeoutMs);
  });
  let run: Awaited<ReturnType<typeof runExample>> | undefined;
  try {
    run = await Promise.race([
      runExample({ source: method.exampleSource, client, onLog: (e) => logs.push(e) }),
      timeout,
    ]);
    await Promise.race([run.promise, timeout]);
    return { id, status: "pass", output: joinLogs(logs) ?? "ok" };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const log = joinLogs(logs);
    return { id, status: "fail", output: log ? `${log}\n${message}` : message };
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    run?.cancel();
  }
}

function joinLogs(logs: LogEntry[]): string | undefined {
  return logs.length === 0 ? undefined : logs.map((l) => l.text).join("\n");
}

/** Run every generated example sequentially, like the playground diagnosis. */
export async function runDiagnosis(client: TrUApiClient): Promise<DiagnosisRow[]> {
  const rows: DiagnosisRow[] = [];
  for (const service of services) {
    for (const method of service.methods) {
      rows.push(await runOne(client, service.name, method));
    }
  }
  return rows;
}
