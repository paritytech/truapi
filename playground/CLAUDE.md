# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Is

An interactive explorer for the TrUAPI, the Host API surface exposed to products running inside the Polkadot Desktop Browser webview. The app must be opened from within a Host environment. It talks to the host over iframe `postMessage` frames or the native webview `window.__HOST_API_PORT__` MessagePort.

To develop locally, run `yarn dev` and open the app via `https://dot.li/localhost:3000` inside the Desktop Host.

## Commands

```bash
yarn dev               # Start Next.js dev server (port 3000)
yarn build             # Build static export to out/
yarn start             # Serve out/ locally
yarn lint              # ESLint + tsc --noEmit + tsc -p tsconfig.examples.json
yarn lint:fix          # Auto-fix ESLint issues
yarn typecheck         # tsc --noEmit on playground sources
yarn typecheck:examples # Typecheck generated client examples
yarn e2e               # Playwright e2e suite
```

The Diagnosis screen emits a per-host markdown report via "Copy report". Aggregation into the cross-host matrix happens in the explorer (see [`../explorer/README.md`](../explorer/README.md#host-compatibility-matrix)). The playground itself owns the diagnosis run and the report format only.

`predev` and `prebuild`/`prelint`/`pretypecheck` automatically run `scripts/bundle-rxjs-dts.mjs` (and, for `predev`, `scripts/write-dev-env.mjs`) so the Monaco editor always has up-to-date rxjs type definitions.

## Architecture

**Stack:** Next.js 15 (static export), React 19, TypeScript, Monaco editor (via `@monaco-editor/react`), rxjs, sucrase.
**Output:** `out/` deployed to DotNS via GitHub Actions on push to `main`. The same workflow also publishes the cargo doc HTML at `/cargo_doc/`.

### Key Files

| File | Role |
| --- | --- |
| `src/lib/services.ts` | Re-exports `services` from `@parity/truapi/playground/services`, which the Rust codegen produces from rustdoc `ts` examples. Read-only. |
| `src/lib/transport.ts` | Singleton `Provider`/`Transport`/`TrUApiClient` over iframe postMessage or webview MessagePort. Owns the handshake and connection status. |
| `src/lib/example-runner.ts` | Transpiles each rustdoc `ts` example via sucrase, runs it inside an `AsyncFunction` with `truapi`, `console`, and rxjs as ambient bindings. A tracking Proxy auto-unsubscribes inner `.subscribe(...)` calls so subscriptions clean up when the user navigates away. |
| `src/lib/monaco-setup.ts` | Configures Monaco's TS worker: registers the bundled `@parity/truapi` types (`truapi-dts`), every rxjs `.d.ts`, and an ambient `declare const truapi: Client` so examples typecheck without manual imports. Defines the light/dark themes that match the design tokens. |
| `src/lib/auto-test.ts` | Runs every method's example (in parallel with a small concurrency budget) and reports pass / fail. `runDiagnosis` runs the full flow: non-disruptive methods in parallel, then each disruptive method sequentially so phone signing can complete one at a time. |
| `src/lib/diagnosis-report.ts` | Renders the diagnosis results as a copy-pasteable GitHub-flavoured markdown table: a `## Truapi <Web\|Desktop\|Android\|iOS> Diagnosis` title (host mode via `detectHostMode` — a native host (Electron UA or `__HOST_WEBVIEW_MARK__`) is split by user-agent into Desktop / Android / iOS, a browser iframe ⇒ Web), a generated timestamp, and one method/status row per method. Consumed by the explorer's matrix aggregator. |
| `src/lib/result-status.ts` | Shared `errorTextFrom` helper. An example's Err surfaces as a `console.error` log or a returned neverthrow Err, not a throw, so both `MethodView` and `auto-test.ts` use this to tell a failed call from a successful one. |
| `src/lib/host-api-bridge.ts` | Just `stringify`, the JSON-with-bigint helper shared across components. |
| `src/components/ExampleEditor.tsx` | Monaco editor wrapper. Auto-folds `// #region helpers` blocks on mount. |
| `src/components/MethodView.tsx` | Per-method view: signature link to cargo doc, Example / Output tabs, status LED, Run / Stop buttons. |
| `src/components/AutoTestView.tsx` | Auto-Test screen: parallel pass/fail run with a "Skip disruptive"/"All methods" toggle and editable per-method retry. |
| `src/components/DiagnosisView.tsx` | Diagnosis screen (own sidebar entry): purpose + login/phone instructions, a Run button, a live per-method log (queued → processing… → success/failed), and the copy-pasteable report. |
| `src/components/ServiceTable.tsx` / `CommandPalette.tsx` | Method browser and ⌘K search. The browser also hosts the Diagnosis and Auto-Test entries. |
| `src/app/page.tsx` | Root: connection status, selection state, deep-link sync via `pushState` + `popstate`. |

### Source of Truth for Methods

`@parity/truapi/playground/services` is generated from the truapi crate's rustdoc JSON. Each method entry carries:

- `name`, `type` (`"unary"` or `"subscription"`)
- `signature`: the TS-shaped method signature shown in the API panel
- `docUrl`: a fragment (e.g. `api/account/trait.Account.html#method.get_account`) joined onto `NEXT_PUBLIC_CARGO_DOC_BASE` to link out to cargo doc
- `description`, `requestDescription`
- `exampleSource`: the `ts` rustdoc block, with `// #region helpers` blocks intact

To add or change a method, edit the trait in `rust/crates/truapi/src/api/*.rs` and rerun `./scripts/codegen.sh` from the repo root. Never edit the generated files directly.

### Call Flow

```
ServiceTable click / CommandPalette / ?service=…&method=… URL
  → page.tsx Selection state (pushState keeps the URL in sync)
    → MethodView mounts ExampleEditor with method.exampleSource
      → user clicks Run
        → example-runner: sucrase transpiles TS → wraps in AsyncFunction
          → executes with ambient `truapi` (a Proxy over the singleton client)
            → unary: awaits the Promise, renders the Result via result.match
            → subscription: returns a tracked Subscription; Stop calls unsubscribe
```

A method without `exampleSource` shows a "Not supported" badge and disables the Run button. Adding support means writing a `ts` rustdoc block on the trait method.

### Transport

`transport.ts` auto-detects environment (iframe vs webview) and exposes singletons `getTransport()` and `getClient()`. The first call to `subscribeConnectionStatus()` triggers a generated `host_handshake` round-trip. The generated package owns both `TRUAPI_VERSION` and `TRUAPI_CODEC_VERSION`, so callers do not pass the codec version manually. Never create multiple transport or client instances.

In iframe mode the playground talks to its parent window via `postMessage` carrying SCALE-encoded `Uint8Array` frames. In webview mode it pulls a `MessagePort` from `window.__HOST_API_PORT__` (set by the native host) and uses `createMessagePortProvider`. The shared `@parity/truapi` transport also answers inbound `host_handshake_request` frames automatically by decoding the inbound versioned wrapper and encoding the matching `HostHandshakeResponse` variant, so V2 hosts receive V2 responses while V1 hosts remain decodable.

### Generated Artifacts

The following directories are gitignored and produced by `./scripts/codegen.sh` plus the playground's `prebuild`/`predev`:

- `src/lib/codegen/rxjs-dts.ts` — every `.d.ts` under `node_modules/rxjs/dist/types/`, snapshot for Monaco.
- `test/generated/examples/` — one file per method, lint-checked via `tsc -p tsconfig.examples.json`.
- `public/cargo_doc/` — `yarn dev` only: symlinks the local `target/doc/truapi/...` under Next's static `public/` so cargo-doc links stay http-origin (Chrome blocks `file://` from `http://`).
- `.env.development.local` — also dev-only, sets `NEXT_PUBLIC_CARGO_DOC_BASE=/cargo_doc`.
