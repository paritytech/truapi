import { transform } from "sucrase";
import type { TrUApiClient } from "@parity/truapi";

export type LogEntry = {
  level: "log" | "error" | "warn";
  text: string;
};

export type RunSubscription = {
  unsubscribe: () => void;
  subscriptionId?: string;
};

export type RunResult =
  | { kind: "unary"; promise: Promise<unknown> }
  | { kind: "subscription"; subscription: RunSubscription };

const IMPORT_RE = /^\s*import\s+[^;]*?from\s+["']@parity\/truapi["'];?\s*$/gm;
// `new Function(...)` can't take ESM `export` declarations. The example is
// inlined into a function body, so we drop the keyword and keep the rest of
// the declaration intact.
const EXPORT_RE =
  /^(\s*)export\s+(async\s+function|function|const|let|var|class)\b/gm;

export class ExampleSyntaxError extends Error {}

type ConsoleShim = {
  log: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
};

export async function runExample(opts: {
  source: string;
  functionName: string;
  client: TrUApiClient;
  onLog: (entry: LogEntry) => void;
}): Promise<RunResult> {
  const { source, functionName, client, onLog } = opts;

  // Monaco supplies the editor (with TS typecheck + intellisense); sucrase
  // strips TS types here so the runner doesn't depend on Monaco's bundled TS
  // worker (which omits `getEmitOutput`).
  let js: string;
  try {
    js = transform(source, { transforms: ["typescript"] }).code;
  } catch (err) {
    throw new ExampleSyntaxError(
      err instanceof Error ? err.message : String(err),
    );
  }

  const stripped = js.replace(IMPORT_RE, "").replace(EXPORT_RE, "$1$2");

  const wrapped =
    "const console = { log: __console.log, error: __console.error, warn: __console.warn };\n" +
    stripped +
    "\nreturn " +
    functionName +
    "(truapi);\n";

  let factory: (truapi: TrUApiClient, __console: ConsoleShim) => unknown;
  try {
    factory = new Function("truapi", "__console", wrapped) as typeof factory;
  } catch (err) {
    throw new ExampleSyntaxError(
      `wrap failed: ${err instanceof Error ? err.message : String(err)}`,
    );
  }

  const consoleShim: ConsoleShim = {
    log: (...args) => onLog({ level: "log", text: format(args) }),
    error: (...args) => onLog({ level: "error", text: format(args) }),
    warn: (...args) => onLog({ level: "warn", text: format(args) }),
  };

  const ret = factory(client, consoleShim);

  if (ret && typeof (ret as { then?: unknown }).then === "function") {
    return { kind: "unary", promise: ret as Promise<unknown> };
  }
  if (
    ret &&
    typeof (ret as { unsubscribe?: unknown }).unsubscribe === "function"
  ) {
    return { kind: "subscription", subscription: ret as RunSubscription };
  }
  throw new Error("example must return Promise or Subscription");
}

function format(args: unknown[]): string {
  return args
    .map((a) =>
      typeof a === "string"
        ? a
        : JSON.stringify(
            a,
            (_, v) => (typeof v === "bigint" ? v.toString() + "n" : v),
            2,
          ),
    )
    .join(" ");
}
