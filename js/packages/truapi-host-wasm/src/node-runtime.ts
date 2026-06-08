import {
  createWasmProvider,
  type TrUApiHostWasmProvider,
  type WasmCoreLike,
  type WasmRawCallbacks,
  type WasmRuntimeConfig,
} from "./runtime.js";

interface NodeWasmModuleShape {
  WasmTrUApiCore: new (callbacks: unknown, runtimeConfig: unknown) => WasmCoreLike;
  setDebugEnabled: (enabled: boolean) => void;
}

/**
 * Options for `createNodeWasmProvider`.
 */
export interface CreateNodeWasmProviderOptions {
  /** Toggle the wasm core's debug logging. Default: `false`. */
  debug?: boolean;
  /** Static product/pairing config passed to the Rust core. */
  runtimeConfig: WasmRuntimeConfig;
}

/**
 * Lazy-load the node-targeted WASM bundle and wrap it in a `Provider`.
 *
 * The bundle initialises synchronously (wasm-pack nodejs target uses
 * `require()` under the hood for the .wasm file), so callers receive
 * a ready-to-use provider once the dynamic import resolves.
 */
export async function createNodeWasmProvider(
  partial: Omit<WasmRawCallbacks, "emitFrame">,
  options: CreateNodeWasmProviderOptions,
): Promise<TrUApiHostWasmProvider> {
  if (!options?.runtimeConfig) {
    throw new Error("runtimeConfig is required");
  }

  // Dynamic import keeps the WASM module out of the package's static
  // dependency graph and out of the tsc rootDir. Indirected through a
  // variable so TS skips the static module-existence check.
  const wasmNodePath = "./wasm/node/truapi_server.js";
  const mod = (await import(
    /* @vite-ignore */ wasmNodePath
  )) as NodeWasmModuleShape | { default: NodeWasmModuleShape };

  const wasm: NodeWasmModuleShape =
    "WasmTrUApiCore" in mod
      ? (mod as NodeWasmModuleShape)
      : (mod.default as NodeWasmModuleShape);

  if (!wasm?.WasmTrUApiCore) {
    throw new Error("Node WASM bundle did not export WasmTrUApiCore");
  }

  wasm.setDebugEnabled?.(options.debug ?? false);

  return createWasmProvider(
    (raw) => new wasm.WasmTrUApiCore(raw, options.runtimeConfig),
    partial,
  );
}
