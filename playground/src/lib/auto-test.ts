import { getMethodBinding, stringify } from "./host-api-bridge";
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

const UNARY_TIMEOUT_MS = 2_000;
const SIGNING_TIMEOUT_MS = 30_000;
const SUBSCRIPTION_TIMEOUT_MS = 6_000;

const CONCURRENCY = 6;
// Each chain-head method depends on a live follow subscription on the host
// side; running the service serially avoids fanning out concurrent follows.
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

const STATEMENT_STORE_SERVICE = "Statement Store";
const STATEMENT_CREATE_PROOF_METHOD = "create_proof";
const STATEMENT_SUBMIT_ID = "Statement Store/submit";

function parseRequest(method: MethodInfo): unknown {
  if (method.noParams) return null;
  try {
    return JSON.parse(method.defaultRequest ?? "{}");
  } catch {
    return null;
  }
}

async function testUnary(
  call: (req: unknown) => Promise<{ ok: boolean; data: unknown }>,
  req: unknown,
  timeoutMs: number,
): Promise<{ result: "pass" | "fail"; output: string }> {
  try {
    const result = await Promise.race([
      call(req),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error(`timed out after ${timeoutMs / 1000}s`)),
          timeoutMs,
        ),
      ),
    ]);
    return {
      result: result.ok ? "pass" : "fail",
      output: stringify(result.data) ?? "null",
    };
  } catch (e) {
    return {
      result: "fail",
      output: e instanceof Error ? e.message : String(e),
    };
  }
}

async function testSubscription(
  subscribe: (
    req: unknown,
    onEvent: (data: unknown) => void,
    onEnd: (error?: Error) => void,
  ) => { unsubscribe: () => void },
  req: unknown,
): Promise<{ result: "pass" | "fail"; output: string }> {
  return new Promise((resolve) => {
    let settled = false;
    let sub: { unsubscribe: () => void } | null = null;

    const settle = (result: "pass" | "fail", output: string) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      try {
        sub?.unsubscribe();
      } catch {
        /* benign */
      }
      resolve({ result, output });
    };

    const timeout = setTimeout(
      () =>
        settle("fail", `timed out after ${SUBSCRIPTION_TIMEOUT_MS / 1000}s`),
      SUBSCRIPTION_TIMEOUT_MS,
    );

    sub = subscribe(
      req,
      (event) => settle("pass", stringify(event) ?? "null"),
      (error) =>
        settle("fail", error ? error.message : "stream ended without events"),
    );
  });
}

// remote_statement_store_submit needs a real proof to verify; the default
// request only carries a placeholder. Generate one via create_proof using its
// default request, round-tripping through stringify+parse so any Uint8Array
// fields arrive at the bridge as { bytes: "0x..." } envelopes.
async function fetchStatementProof(services: ServiceInfo[]): Promise<unknown> {
  const proofMethod = services
    .find((s: ServiceInfo) => s.name === STATEMENT_STORE_SERVICE)
    ?.methods.find((m: MethodInfo) => m.name === STATEMENT_CREATE_PROOF_METHOD);
  if (!proofMethod) return undefined;

  const binding = getMethodBinding(
    STATEMENT_STORE_SERVICE,
    STATEMENT_CREATE_PROOF_METHOD,
  );
  if (!binding || binding.isStream) return undefined;

  const result = await binding.call(parseRequest(proofMethod));
  if (!result.ok) return undefined;

  return JSON.parse(stringify(result.data));
}

type RunOneOpts = {
  services: ServiceInfo[];
  serviceName: string;
  method: MethodInfo;
  onUpdate: (id: string, entry: TestEntry) => void;
  excludeSet: Set<string>;
  signal?: AbortSignal;
  requestOverride?: string;
};

async function runOne({
  services,
  serviceName,
  method,
  onUpdate,
  excludeSet,
  signal,
  requestOverride,
}: RunOneOpts): Promise<void> {
  if (signal?.aborted) return;

  const id = `${serviceName}/${method.name}`;

  if (excludeSet.has(id)) {
    onUpdate(id, { status: "skipped" });
    return;
  }

  const binding = getMethodBinding(serviceName, method.name);
  if (!binding) {
    onUpdate(id, { status: "skipped" });
    return;
  }

  onUpdate(id, { status: "running" });

  let req: unknown;
  if (requestOverride !== undefined) {
    try {
      req = JSON.parse(requestOverride);
    } catch (e) {
      onUpdate(id, {
        status: "fail",
        request: requestOverride,
        output: `Invalid JSON: ${e instanceof Error ? e.message : String(e)}`,
      });
      return;
    }
  } else {
    req = parseRequest(method);
    if (id === STATEMENT_SUBMIT_ID) {
      const proof = await fetchStatementProof(services);
      if (
        proof !== undefined &&
        typeof req === "object" &&
        req !== null &&
        !Array.isArray(req)
      ) {
        req = { ...req, proof };
      }
    }
  }

  const timeoutMs = LONG_TIMEOUT_METHODS.has(id)
    ? SIGNING_TIMEOUT_MS
    : UNARY_TIMEOUT_MS;
  const requestStr = stringify(req);
  const { result, output } = binding.isStream
    ? await testSubscription(binding.subscribe, req)
    : await testUnary(binding.call, req, timeoutMs);

  onUpdate(id, { status: result, request: requestStr, output });
}

export async function runSingleTest(
  services: ServiceInfo[],
  serviceName: string,
  methodName: string,
  onUpdate: (id: string, entry: TestEntry) => void,
  requestOverride?: string,
): Promise<void> {
  const svc = services.find((s: ServiceInfo) => s.name === serviceName);
  const method = svc?.methods.find((m: MethodInfo) => m.name === methodName);
  if (!svc || !method) return;
  // Empty exclude set so a manual retry overrides the disruptive-method filter.
  await runOne({
    services,
    serviceName,
    method,
    onUpdate,
    excludeSet: new Set(),
    requestOverride,
  });
}

export async function runAutoTests(
  services: ServiceInfo[],
  onUpdate: (id: string, entry: TestEntry) => void,
  signal?: AbortSignal,
  excludeSet: Set<string> = EXCLUDED_METHODS,
): Promise<void> {
  // Build the task list. Serial services bundle their methods into a single
  // sequential task; other services contribute one task per method.
  const tasks: Array<() => Promise<void>> = [];
  for (const svc of services) {
    if (SERIAL_SERVICES.has(svc.name)) {
      tasks.push(async () => {
        for (const m of svc.methods) {
          if (signal?.aborted) return;
          await runOne({
            services,
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
            services,
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

  // Bounded-concurrency worker pool: each worker pulls the next task off the
  // shared cursor until they're exhausted or the run is aborted.
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
