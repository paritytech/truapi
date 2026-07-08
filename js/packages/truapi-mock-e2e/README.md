# @parity/truapi-mock-e2e

Browser E2E harness for the TrUAPI mock host. It proves, **in a real browser**,
that a product runs against the mocked host with the **real Rust core (WASM)** —
no device, no wallet, no phone. Private workspace package; not published.

Unlike the headless `wasm-bridge.test.ts` in `@parity/truapi-host-wasm`, this
exercises the full production transport: the real core in a Web Worker, the
product in an **iframe** connecting via the SDK's real `getClientSync()` sandbox
path over a real `MessageChannel`. Nothing hand-rolls the wire protocol.

## Topologies

- **Single page** (`/` → `src/main.tsx`): host + product in one page; the whole
  setup is a single `createMockClient(new HostWorker())` call
  (`@parity/truapi-host-wasm/testing`).
- **Iframe** (`/host.html` + `/product.html`): mirrors production embedding. The
  host boots the core + `createIframeHost`; the product runs in an iframe with
  **no mock code**, connecting through `getClientSync()`.

## Prerequisites

The workspace packages must be built first (their `dist/` present, including the
WASM bundle):

```bash
# from the repo root
make wasm
( cd js/packages/truapi && npm run build )
( cd js/packages/truapi-host-wasm && npm run build )
```

## Run

```bash
cd js/packages/truapi-mock-e2e
npm run dev            # http://localhost:4319/          (single page)
                       # http://localhost:4319/host.html (iframe topology)
npm run test:e2e       # headless Playwright: iframe topology, asserts each call
```

## What "mock mode" means

The product never talks to a device. Its transport points at the mock host
instead of the real one — the product's own call code is unchanged. Every call
flows through the real dispatcher; the mock only decides what "the device" says.
This browser harness exercises the platform-seam calls (storage, permissions,
features, navigation, notifications, preimage, theme). Signing/login are **not**
driven here — that coverage lives in the Rust through-core tests (PR #258), where
the mock wallet completes login and signing in-process (deterministic and valid
in-process, but **not chain-valid** — a real on-chain signature needs a genuine
signer / signer-bot).
