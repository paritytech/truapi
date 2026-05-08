# TrUAPI Playground

*Browse, edit, and call every TrUAPI method live against a connected Polkadot host.*

The playground is an interactive reference for the TrUAPI: every method grouped by domain, with live request payload editing, one-click calls, and live subscriptions. It must be opened from inside a TrUAPI host so it can talk to the host over the wire.

**Live app:** [https://truapi-playground.dot.li/](https://truapi-playground.dot.li/)

## Features

- **Full method browser**: every TrUAPI service and method, each with a description and a Request / Response or Subscription badge.
- **Live calls**: edit a JSON request payload and fire the call against the connected host.
- **Subscriptions**: open and close streaming methods and watch events arrive in real time.
- **Auto-test view**: runs every wired method and reports pass / fail in one pass.
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
