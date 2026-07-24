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

// Starts from the playground diagnosis policy. The headless transport handles
// product calls serially, so prompt-backed calls must outlive the runtime's
// 180s remote-response timeout. Otherwise a client timeout leaves the transport
// occupied and makes every following example look like a failure.
const UNARY_TIMEOUT_MS = 10_000;
const REMOTE_RESPONSE_TIMEOUT_MS = 190_000;
const LIVE_ALLOCATION_TIMEOUT_MS = 420_000;
const SKIPPED_SERVICES = new Set(["Chat", "Coin Payment", "Payment"]);
const SKIPPED_METHODS = new Set(["Account/create_account_proof"]);
const LONG_TIMEOUT_METHODS = new Set([
  "Account/get_account",
  "Account/get_account_alias",
  "Account/create_account_proof",
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
  ["Account/get_account_alias", REMOTE_RESPONSE_TIMEOUT_MS],
  ["Resource Allocation/request", LIVE_ALLOCATION_TIMEOUT_MS],
  ["Preimage/lookup_subscribe", LIVE_ALLOCATION_TIMEOUT_MS],
  ["Preimage/submit", LIVE_ALLOCATION_TIMEOUT_MS],
  ["Signing/create_transaction", REMOTE_RESPONSE_TIMEOUT_MS],
  ["Statement Store/create_proof_authorized", LIVE_ALLOCATION_TIMEOUT_MS],
  ["Statement Store/submit", LIVE_ALLOCATION_TIMEOUT_MS],
  ["Statement Store/subscribe", LIVE_ALLOCATION_TIMEOUT_MS],
]);

export type DiagnosisStatus = "pass" | "fail" | "skipped";
export interface DiagnosisRow {
  id: string;
  serviceName: string;
  methodName: string;
  status: DiagnosisStatus;
  output: string;
  durationMs: number;
}

export interface DiagnosisCase {
  id: string;
  serviceName: string;
  methodName: string;
  exampleSource?: string;
  skipReason?: string;
}

export interface DiagnosisOptions {
  /** Attempt examples the playground normally labels as intentionally unsupported. */
  runKnownUnsupported?: boolean;
  onStart?: (test: DiagnosisCase, index: number, total: number) => void;
  onResult?: (row: DiagnosisRow, index: number, total: number) => void;
}

/** Build the battery directly from the generated playground service manifest. */
export function createDiagnosisPlan(
  options: Pick<DiagnosisOptions, "runKnownUnsupported"> = {},
): DiagnosisCase[] {
  return services.flatMap((service) =>
    service.methods.map((method) => {
      const id = `${service.name}/${method.name}`;
      return {
        id,
        serviceName: service.name,
        methodName: method.name,
        exampleSource: method.exampleSource,
        skipReason: options.runKnownUnsupported
          ? undefined
          : knownSkipReason(service.name, id),
      };
    }),
  );
}

function knownSkipReason(serviceName: string, id: string): string | undefined {
  if (SKIPPED_SERVICES.has(serviceName)) {
    return `${serviceName} service not yet wired up by hosts`;
  }
  if (SKIPPED_METHODS.has(id)) return "host surface intentionally deferred";
  return undefined;
}

async function runOne(
  client: TrUApiClient,
  test: DiagnosisCase,
): Promise<DiagnosisRow> {
  const startedAt = performance.now();
  const finish = (status: DiagnosisStatus, output: string): DiagnosisRow => ({
    id: test.id,
    serviceName: test.serviceName,
    methodName: test.methodName,
    status,
    output,
    durationMs: Math.round(performance.now() - startedAt),
  });

  if (test.skipReason) {
    return finish("skipped", test.skipReason);
  }
  if (!test.exampleSource) {
    return finish("fail", "no runnable example");
  }
  const timeoutMs =
    METHOD_TIMEOUT_MS.get(test.id) ??
    (LONG_TIMEOUT_METHODS.has(test.id)
      ? REMOTE_RESPONSE_TIMEOUT_MS
      : UNARY_TIMEOUT_MS);

  const logs: LogEntry[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`timed out after ${timeoutMs / 1000}s`)),
      timeoutMs,
    );
  });
  let run: Awaited<ReturnType<typeof runExample>> | undefined;
  try {
    run = await Promise.race([
      runExample({
        source: test.exampleSource,
        client,
        onLog: (event) => logs.push(event),
      }),
      timeout,
    ]);
    await Promise.race([run.promise, timeout]);
    return finish("pass", joinLogs(logs) ?? "ok");
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const log = joinLogs(logs);
    return finish("fail", log ? `${log}\n${message}` : message);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    run?.cancel();
  }
}

function joinLogs(logs: LogEntry[]): string | undefined {
  return logs.length === 0 ? undefined : logs.map((l) => l.text).join("\n");
}

/** Run every generated example sequentially, like the playground diagnosis. */
export async function runDiagnosis(
  client: TrUApiClient,
  options: DiagnosisOptions = {},
): Promise<DiagnosisRow[]> {
  const plan = createDiagnosisPlan(options);
  const rows: DiagnosisRow[] = [];
  for (const [index, test] of plan.entries()) {
    options.onStart?.(test, index, plan.length);
    const row = await runOne(client, test);
    rows.push(row);
    options.onResult?.(row, index, plan.length);
  }
  return rows;
}
