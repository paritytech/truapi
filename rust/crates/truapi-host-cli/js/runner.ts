// Host-script runner: the Rust CLI spawns this to drive a headless host from a
// user-provided JavaScript/TypeScript file.
//
// The pairing host serves the product frame protocol on a WebSocket; this
// runner connects the real `@parity/truapi` client to it, injects it as the
// global `truapi` (scoped to the host's product id), and evaluates the user
// script. The script is the product: it calls `truapi.account.requestLogin()`,
// `truapi.signing.*`, `truapi.localStorage.*`, etc. A thrown error or rejected
// promise exits non-zero, so `truapi-host pairing-host --script …` is the test.
//
// Env (set by the Rust CLI):
//   TRUAPI_FRAME_URL   ws:// URL of the pairing host's frame server
//   TRUAPI_PRODUCT_ID  product id the host serves (scopes storage etc.)
//   TRUAPI_SCRIPT      absolute path to the user script
import { pathToFileURL } from "node:url";
import {
  createClient,
  createTransport,
  type ProductAccountId,
  type TrUApiClient,
} from "../../../../js/packages/truapi/src/index.ts";
import { wsProvider } from "./ws-provider.ts";

/** Helpers injected alongside `truapi` for scripts to use. */
export interface HostContext {
  /** The product id this host serves. */
  productId: string;
  /** A product account id for `derivationIndex` (default 0) under this product. */
  productAccount(index?: number): ProductAccountId;
  /** Log to stderr (keeps stdout clean for machine-readable host output). */
  log(...args: unknown[]): void;
  /** Throw `message` unless `condition` holds. */
  assert(condition: unknown, message: string): asserts condition;
}

declare global {
  // eslint-disable-next-line no-var
  var truapi: TrUApiClient;
  // eslint-disable-next-line no-var
  var host: HostContext;
}

const OPEN_TIMEOUT_MS = 15_000;

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} must be set`);
  return value;
}

async function main() {
  const frameUrl = requireEnv("TRUAPI_FRAME_URL");
  const productId = requireEnv("TRUAPI_PRODUCT_ID");
  const scriptPath = requireEnv("TRUAPI_SCRIPT");

  const provider = wsProvider(frameUrl);
  const client = createClient(createTransport(provider));

  const context: HostContext = {
    productId,
    productAccount: (index = 0) => ({ dotNsIdentifier: productId, derivationIndex: index }),
    log: (...args) => console.error("[script]", ...args),
    assert: (condition, message) => {
      if (!condition) throw new Error(`assertion failed: ${message}`);
    },
  };
  globalThis.truapi = client;
  globalThis.host = context;

  const timer = setTimeout(() => {
    console.error(`[runner] timed out connecting to ${frameUrl}`);
    process.exit(2);
  }, OPEN_TIMEOUT_MS);
  await provider.opened;
  clearTimeout(timer);

  try {
    const module = await import(pathToFileURL(scriptPath).href);
    if (typeof module.default === "function") {
      await module.default(context);
    }
  } finally {
    provider.dispose();
  }
}

main().then(
  () => process.exit(0),
  (error) => {
    console.error(`[script error] ${error instanceof Error ? error.stack : String(error)}`);
    process.exit(1);
  },
);
