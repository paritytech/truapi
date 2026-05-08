# TrUAPI Playground

An interactive explorer for the TrUAPI, the API surface exposed to products running inside the Polkadot Desktop Browser webview.

Browse all available methods, read their descriptions, edit request payloads, and call or subscribe to them directly from the browser.

**Live app:** [https://truapi-playground.dot.li/](https://truapi-playground.dot.li/)

### Local Development

1. Install dependencies and start the dev server:

```bash
yarn install --frozen-lockfile
yarn dev
```

2. Open the your browser and navigate to:

```
https://dot.li/localhost:3000
```

## Features

- Browse all configured TrUAPI methods grouped by category
- Each method shows its description and type badge (Request / Response or Subscription)
- Methods not yet wired to a binding are marked as "Not supported"
- Auto-test view that executes every wired method and reports pass/fail results
- Descriptions and request type hints sourced

## Adding a New Method

1. Add an entry to `src/lib/services.ts` with `name`, `type`, `description`, and optionally `defaultRequest`, `requestDescription`, or `noParams: true`.
2. Add a corresponding entry to `methodMap` in `src/lib/host-api-bridge.ts` mapping `"ServiceName/method_name"` to `[hostApiMethodName, isStream]`.

If step 2 is omitted the method will appear in the UI with a "Not supported" badge until it is wired up.

## Deployment

Pushes to `main` are deployed automatically to DotNS via the [Deploy workflow](.github/workflows/deploy.yaml). The steps below mirror that workflow and can be used to publish from your local machine when you need to ship out-of-band (for example, while CI is unavailable or to test a branch against the live DotNS name).

### Prerequisites

- Node.js 22 (matches CI).
- `bulletin-deploy` installed globally:

```bash
npm install -g bulletin-deploy
```

### Deploy from local

1. Install dependencies and build the static export:

```bash
yarn install --frozen-lockfile
yarn build
```

The build output will be located at `./out`

2. Publish `out/` to the `truapi-playground.dot` DotNS name:

```bash
bulletin-deploy ./out truapi-playground.dot --js-merkle
```

The deploy can occasionally fail on transient network errors — CI retries up to 3 times. If it fails locally, just re-run the command.

### Shortcut

A `deploy:test` script is provided for quick iteration against the same DotNS name (it omits `--js-merkle` and cleans up the generated `out.car`):

```bash
yarn deploy:test
```
