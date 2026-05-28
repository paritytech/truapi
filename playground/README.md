# TrUAPI Playground

_Browse, edit, and call every TrUAPI method live against a connected Polkadot host._

The playground is an interactive reference for the TrUAPI: every method grouped by domain, with live request payload editing, one-click calls, and live subscriptions. It must be opened from inside a TrUAPI host so it can talk to the host over the wire.

**Live app:** [https://truapi-playground.dot.li/](https://truapi-playground.dot.li/)

## Features

- **Full method browser**: every TrUAPI service and method, each with a description and a Request / Response or Subscription badge.
- **Live calls**: edit a JSON request payload and fire the call against the connected host.
- **Subscriptions**: open and close streaming methods and watch events arrive in real time.
- **Auto-test view**: runs every method and reports pass / fail in one pass.
- **Run diagnosis**: runs the full surface — non-disruptive methods in parallel, then each disruptive method (signing, permission/resource requests, `navigate_to`) sequentially so you can complete phone interactions one at a time — and produces a copy-pasteable markdown table (`## Truapi Web/Desktop Diagnosis`, a timestamp, and a method/status row per method) that feeds the cross-host compatibility matrix.

Collect one diagnosis report per host, then merge them into a single host × method matrix. Drop each report (any `*.md` filename) into `pending-reports/` and run:

```bash
yarn generate-matrix   # or: npm run generate-matrix
```

This consumes every report in `pending-reports/` (deleting them) and writes the combined `matrix.md` to the playground root. Columns are the hosts (the `Web`/`Desktop` mode from each report's title; same-mode reports are disambiguated by filename), rows are the methods, and a method missing from a report shows `—`.

The underlying script also runs standalone for ad-hoc use:

```bash
node ../scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md > matrix.md
node ../scripts/aggregate-diagnosis-matrix.mjs reports/   # all *.md in a dir
```
- **Wiring status**: methods that are not yet bound are flagged "Not supported" so you can see protocol coverage at a glance.

## Local development

```bash
yarn install --frozen-lockfile
yarn dev
```

Then open the dev server inside the Polkadot Desktop Host:

```
https://dot.li/localhost:3000
```

The app needs a host to connect to. Opening it directly in a regular browser will not work.

## Adding a method

1. Add the method to the Rust API contract and run [`../scripts/codegen.sh`](../scripts/codegen.sh). The generated playground metadata is re-exported from [`src/lib/services.ts`](src/lib/services.ts).
2. Add a corresponding entry to `methodMap` in [`src/lib/host-api-bridge.ts`](src/lib/host-api-bridge.ts), mapping `"ServiceName/method_name"` to `[serviceField, clientMethod, isStream]` on the generated `@parity/truapi` client.

If you skip step 2, the method shows up in the UI with a "Not supported" badge until it is wired.

## Deploy

Pushes to `main` deploy automatically via the [Deploy Playground workflow](../.github/workflows/deploy-playground.yml). The steps below mirror that workflow and let you ship out-of-band, for example to test a branch against the live DotNS name.

### Prerequisites

- Node.js 22 (matches CI).
- `bulletin-deploy` installed globally: `npm install -g bulletin-deploy`.

### Deploy from local

```bash
yarn install --frozen-lockfile
yarn build
bulletin-deploy ./out truapi-playground.dot --js-merkle
```

The build output goes to `./out`. The deploy can fail on transient network errors; CI retries up to 3 times, and you can simply rerun the command locally.

### Quick iteration

`deploy:test` skips `--js-merkle` and cleans up the generated `out.car`:

```bash
yarn deploy:test
```

## License

[MIT](../LICENSE)
