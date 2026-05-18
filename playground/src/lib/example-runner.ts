import { transform } from "sucrase";
import type { Subscription, TrUApiClient } from "@parity/truapi";

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
const EXPORT_RE =
  /^(\s*)export\s+(async\s+function|function|const|let|var|class)\b/gm;

export class ExampleSyntaxError extends Error {}

type ConsoleShim = {
  log: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
};

const AsyncFunction = Object.getPrototypeOf(
  async function () {},
).constructor as new (...args: string[]) => (
  truapi: unknown,
  __console: ConsoleShim,
) => Promise<unknown>;

export async function runExample(opts: {
  source: string;
  kind: "unary" | "subscription";
  client: TrUApiClient;
  onLog: (entry: LogEntry) => void;
}): Promise<RunResult> {
  const { source, kind, client, onLog } = opts;

  let js: string;
  try {
    js = transform(source, { transforms: ["typescript"] }).code;
  } catch (err) {
    throw new ExampleSyntaxError(
      err instanceof Error ? err.message : String(err),
    );
  }

  const stripped = js.replace(IMPORT_RE, "").replace(EXPORT_RE, "$1$2");
  const body = `const console = __console;\n${stripped}`;

  let run: (truapi: unknown, c: ConsoleShim) => Promise<unknown>;
  try {
    run = new AsyncFunction("truapi", "__console", body);
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

  const tracked: Subscription[] = [];
  const trackingClient = createTrackingClient(client, (sub) =>
    tracked.push(sub),
  );

  const promise = run(trackingClient, consoleShim);

  if (kind === "subscription") {
    await promise;
    return {
      kind: "subscription",
      subscription: {
        unsubscribe: () => {
          for (const sub of tracked) {
            try {
              sub.unsubscribe();
            } catch {
              /* benign */
            }
          }
        },
        subscriptionId: tracked[0]?.subscriptionId,
      },
    };
  }

  return { kind: "unary", promise };
}

function createTrackingClient(
  client: TrUApiClient,
  onSub: (sub: Subscription) => void,
): unknown {
  return new Proxy(client as unknown as Record<string, unknown>, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (!isPlainServiceObject(value)) return value;
      return wrapService(value, onSub);
    },
  });
}

function wrapService(
  svc: object,
  onSub: (sub: Subscription) => void,
): unknown {
  return new Proxy(svc as Record<string, unknown>, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);
      if (typeof value !== "function") return value;
      return (...args: unknown[]) => {
        const out = (value as (...a: unknown[]) => unknown).apply(target, args);
        if (out && typeof (out as { subscribe?: unknown }).subscribe === "function") {
          return wrapObservable(out as ObservableLike, onSub);
        }
        return out;
      };
    },
  });
}

type ObservableLike = {
  subscribe: (...args: unknown[]) => Subscription;
};

function wrapObservable(
  observable: ObservableLike,
  onSub: (sub: Subscription) => void,
): ObservableLike {
  return new Proxy(observable, {
    get(target, prop, receiver) {
      if (prop !== "subscribe") return Reflect.get(target, prop, receiver);
      return (...args: unknown[]) => {
        const sub = target.subscribe(...args);
        onSub(sub);
        return sub;
      };
    },
  });
}

function isPlainServiceObject(value: unknown): value is object {
  return (
    typeof value === "object" &&
    value !== null &&
    !(value instanceof Promise) &&
    typeof (value as { subscribe?: unknown }).subscribe !== "function"
  );
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
