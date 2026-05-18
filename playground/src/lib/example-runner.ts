import type { Monaco } from "@monaco-editor/react";
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

export class ExampleSyntaxError extends Error {}

type ConsoleShim = {
  log: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
};

export async function runExample(opts: {
  monaco: Monaco;
  source: string;
  functionName: string;
  uri: string;
  client: TrUApiClient;
  onLog: (entry: LogEntry) => void;
}): Promise<RunResult> {
  const { monaco, source, functionName, uri, client, onLog } = opts;

  const modelUri = monaco.Uri.parse(uri);
  const existing = monaco.editor.getModel(modelUri);
  if (existing) {
    if (existing.getValue() !== source) existing.setValue(source);
  } else {
    monaco.editor.createModel(source, "typescript", modelUri);
  }

  const workerFactory = await monaco.languages.typescript.getTypeScriptWorker();
  const worker = await workerFactory(modelUri);
  const emit = await worker.getEmitOutput(modelUri.toString());
  if (emit.emitSkipped || emit.outputFiles.length === 0) {
    throw new ExampleSyntaxError("Monaco TS worker did not emit JS output");
  }
  const jsFile = emit.outputFiles.find((f) => f.name.endsWith(".js"));
  if (!jsFile) throw new ExampleSyntaxError("no .js output file from worker");

  const stripped = jsFile.text.replace(IMPORT_RE, "");

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
