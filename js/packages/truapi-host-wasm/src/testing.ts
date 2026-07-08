// One-call setup for driving a product against the mock host. It collapses
// `createMockHost` + `createWebWorkerPairingHostRuntime` + `createClient` into a
// single call, so a test (or a product's "mock mode") gets a ready-to-use client
// in one line. Experimental: the shape may change as more product test-cases
// adopt it.
import { createClient, createTransport, type TrUApiClient } from "@parity/truapi";
import { createMockHost, createWebWorkerPairingHostRuntime, mockRuntimeConfig } from "./web/index.js";
import type { MockHost, MockHostConfig, WebWorkerHostCallbacks } from "./web/index.js";

/** A product client wired to the mock host, plus the mock for assertions. */
export interface MockClient {
  /** The product SDK client — the exact object a product uses in production. */
  client: TrUApiClient;
  /** The mock host, exposing recorded oracles (navigations, confirmations, …). */
  mock: MockHost;
}

/** Lift the mock's flat callbacks into the namespaced `WebWorkerHostCallbacks`
 *  (`RequiredHostCallbacks`) shape the worker runtime type expects.
 *  `createWasmRawCallbacks` normalizes the flat shape at runtime, but the runtime
 *  factory's parameter is typed namespaced, so each capability slot points at the
 *  one flat object. */
function namespaceMockCallbacks(flat: MockHost["callbacks"]): WebWorkerHostCallbacks {
  return {
    navigation: flat,
    notifications: flat,
    permissions: flat,
    features: flat,
    productStorage: flat,
    coreStorage: flat,
    chain: flat,
    auth: flat,
    userConfirmation: flat,
    theme: flat,
    preimage: flat,
  };
}

/**
 * Wire a product {@link TrUApiClient} to the real WASM core running against the
 * mock host, in a single call.
 *
 * Pass the core Worker — e.g. `new HostWorker()` from
 * `@parity/truapi-host-wasm/worker-runtime?worker` — so the caller's bundler owns
 * how the worker is produced. The returned `client` talks to the real dispatcher
 * over the mocked platform seam; `mock` lets a test assert on what the core asked
 * the device to do.
 */
export async function createMockClient(worker: Worker, config?: MockHostConfig): Promise<MockClient> {
  const mock = createMockHost(config);
  const { productId, ...hostConfig } = mockRuntimeConfig();
  // createMockHost implements every callback (its impl is `Required<FlatHostCallbacks>`),
  // so the namespaced lift satisfies the runtime's stricter callback surface.
  const runtime = await createWebWorkerPairingHostRuntime(worker, namespaceMockCallbacks(mock.callbacks), {
    hostConfig,
  });
  const provider = await runtime.createProvider({ productId });
  return { client: createClient(createTransport(provider)), mock };
}
