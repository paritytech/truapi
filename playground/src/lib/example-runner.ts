import { transform } from "sucrase";
import type { Subscription, TrUApiClient } from "@parity/truapi";
import { createWithChainHeadFollow, type WithChainHeadFollow } from "./example-helpers";

export type LogEntry = {
  level: "log" | "error" | "warn";
  text: string;
};

export type RunSubscription = {
  unsubscribe: () => void;
  subscriptionId?: string;
};

export type RunResult =
  | { kind: "unary"; promise: Promise<unknown>; cancel: () => void }
  | { kind: "subscription"; subscription: RunSubscription };

// Drop any `@parity/truapi` import that does not name value specifiers (e.g.
// bare type-only imports left over after sucrase). Named value imports are
// rewritten by `TRUAPI_NAMED_IMPORT_RE` below.
const IMPORT_RE = /^\s*import\s+(?!\{)[^;]*?from\s+["']@parity\/truapi["'];?\s*$/gm;
// `import { PASEO_NEXT_V2_ASSET_HUB, ... } from "@parity/truapi"`
//   → `const { PASEO_NEXT_V2_ASSET_HUB, ... } = __truapi;`
const TRUAPI_NAMED_IMPORT_RE =
  /^\s*import\s*(\{[^}]*\})\s*from\s+["']@parity\/truapi["'];?\s*$/gm;
// `import { from, take, ... } from "rxjs"` → `const { from, take, ... } = __rxjs;`
const RXJS_IMPORT_RE =
  /^\s*import\s*(\{[^}]*\})\s*from\s+["']rxjs["'];?\s*$/gm;
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
  __rxjs: unknown,
  withChainHeadFollow: WithChainHeadFollow,
  __truapi: unknown,
) => Promise<unknown>;

function lazyImport(load: () => Promise<unknown>): () => Promise<unknown> {
  let promise: Promise<unknown> | null = null;
  return () => (promise ??= load());
}
const getRxjs = lazyImport(() => import("rxjs"));
const getTruapiPkg = lazyImport(() => import("@parity/truapi"));

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

  const stripped = js
    .replace(TRUAPI_NAMED_IMPORT_RE, "const $1 = __truapi;")
    .replace(IMPORT_RE, "")
    .replace(RXJS_IMPORT_RE, "const $1 = __rxjs;")
    .replace(EXPORT_RE, "$1$2");
  const body = `const console = __console;\n${stripped}`;

  let run: (
    truapi: unknown,
    c: ConsoleShim,
    rxjs: unknown,
    withChainHeadFollow: WithChainHeadFollow,
    truapiPkg: unknown,
  ) => Promise<unknown>;
  try {
    run = new AsyncFunction(
      "truapi",
      "__console",
      "__rxjs",
      "withChainHeadFollow",
      "__truapi",
      body,
    );
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

  const unsubscribeAll = () => {
    for (const sub of tracked) {
      try {
        sub.unsubscribe();
      } catch {
        /* benign */
      }
    }
  };

  const [rxjs, truapiPkg] = await Promise.all([getRxjs(), getTruapiPkg()]);
  const withChainHeadFollow = createWithChainHeadFollow(trackingClient as TrUApiClient);
  const promise = run(
    trackingClient,
    consoleShim,
    rxjs,
    withChainHeadFollow,
    truapiPkg,
  );

  if (kind === "subscription") {
    await promise;
    return {
      kind: "subscription",
      subscription: {
        unsubscribe: unsubscribeAll,
        subscriptionId: tracked[0]?.subscriptionId,
      },
    };
  }

  promise.finally(unsubscribeAll);
  return { kind: "unary", promise, cancel: unsubscribeAll };
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
