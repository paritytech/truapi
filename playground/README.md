# TrUAPI Playground

_Browse, edit, and call every TrUAPI method live against a connected Polkadot host._

The playground is an interactive reference for the TrUAPI: every method grouped by domain, with live request payload editing, one-click calls, and live subscriptions. It must be opened from inside a TrUAPI host so it can talk to the host over the wire.

**Live app:** [https://truapi-playground.dot.li/](https://truapi-playground.dot.li/)

## Features

- **Full method browser**: every TrUAPI service and method, each with a description and a Request / Response or Subscription badge.
- **Live calls**: edit a JSON request payload and fire the call against the connected host.
- **Subscriptions**: open and close streaming methods and watch events arrive in real time.
- **Auto-test view**: runs every method and reports pass / fail in one pass.
- **Diagnosis view**: runs the full surface and produces a copy-pasteable markdown table that feeds the cross-host compatibility matrix. See [Diagnosis & compatibility matrix](#diagnosis--compatibility-matrix).
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

Methods reach the playground via codegen — there is no per-method wiring file to edit. The flow:

1. Edit the trait in [`rust/crates/truapi/src/api/<service>.rs`](../rust/crates/truapi/src/api/) and include a ` ```ts ` rustdoc block on the method. That block becomes the playground's runnable example (the editor contents you see in the **Example** tab).
2. From the repo root, run [`../scripts/codegen.sh`](../scripts/codegen.sh). This regenerates the TS client, the playground metadata re-exported from [`src/lib/services.ts`](src/lib/services.ts), and the per-method example files under `test/generated/examples/`.
3. Rebuild the client and refresh the playground's `file:` snapshot of it:

   ```bash
   ( cd ../js/packages/truapi && npm run build )
   ( cd . && rm -rf node_modules/@parity && yarn install )
   ```

A method without a `ts` rustdoc block shows up with a "Not supported" badge — there is no example to run until you add one.

## Diagnosis & compatibility matrix

The Diagnosis view exercises every TrUAPI method against the connected host and emits a per-host report. Aggregating one report per host gives you a single **host × method** matrix — MDN-browser-compat style — that shows which methods work on which hosts.

There are two roles in this workflow:

1. **Tester** — runs the diagnosis inside a host (web or desktop) and copies the report.
2. **Aggregator** — collects the per-host reports and merges them into the matrix.

### 1. Run a diagnosis inside a host (tester)

Open the playground inside a TrUAPI host (it cannot run standalone in a browser tab):

- **Web host:** [https://truapi-playground.dot.li/](https://truapi-playground.dot.li/) opened inside dot.li.
- **Desktop host:** the Polkadot Desktop app pointed at the playground URL.

Before you start:

- Make sure you are **logged in** to the host.
- Keep your **phone nearby** — the disruptive methods (signing, permission requests) will prompt the Polkadot mobile app and the diagnosis will wait for you to approve each one.

Then, in the playground:

1. Click **Diagnosis** in the left sidebar (below Auto-Test, above the service list).
2. Read the instructions on the screen, then click **Run diagnosis**.
3. Wait for the run to finish. Non-disruptive methods run in parallel first, then disruptive methods run one at a time — approve each pop-up on your phone as it appears. A live log updates per method (`queued → processing… → success / failed`).
4. When the run finishes, a **Report** panel appears above the log. Click **Copy report**.
5. Save the markdown to a file named after the host, e.g. `web.md` or `desktop.md`.

The report looks like this:

```markdown
## Truapi Web Diagnosis
_Generated: 2026-05-28T10:15:00.000Z_

| Method | Status |
| --- | --- |
| `Account/get_account` | ✅ |
| `Account/get_account_alias` | ❌ |
| `System/handshake` | ✅ |
...
```

| Icon | Meaning |
| --- | --- |
| ✅ | The method ran and returned a successful result. |
| ❌ | The method failed — it errored at runtime, the host returned an error, or it has no runnable example yet. |

The host mode in the title (`Web` / `Desktop`) is detected automatically — Electron in the user-agent or the native-webview marker ⇒ Desktop, browser iframe ⇒ Web.

### 2. Generate the matrix (aggregator)

Collect one report per host. Drop each `*.md` file into [`pending-reports/`](pending-reports/) at the playground root and run:

```bash
yarn generate-matrix
```

This consumes every report in `pending-reports/` (deleting them on success) and writes a combined `matrix.md` to the playground root. The output looks like:

```markdown
# TrUAPI Host Compatibility Matrix
_Generated: 2026-05-28T10:30:00.000Z — aggregated from 2 report(s)_

| Method | Web | Desktop |
| --- | --- | --- |
| `Account/get_account` | ❌ | ✅ |
| `Account/get_account_alias` | ❌ | ❌ |
| `System/handshake` | ✅ | ✅ |
...
```

- **Columns** are the hosts, labelled by the mode from each report's title. If two reports share a mode (e.g. two different web hosts) they are disambiguated by their filename.
- **Rows** are the methods, in the order they appear across the reports (union, first-seen-first).
- A method missing from a report renders as `—`.

`pending-reports/*.md` and `matrix.md` are gitignored, so neither the inputs nor the aggregate are checked in by accident.

#### Standalone CLI

The aggregator also runs directly for ad-hoc use:

```bash
node ../scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md > matrix.md
node ../scripts/aggregate-diagnosis-matrix.mjs reports/   # all *.md in a dir
node ../scripts/aggregate-diagnosis-matrix.mjs --out matrix.md --consume pending-reports
```

Flags:

- `--out <file>` — write the matrix to `<file>` instead of stdout.
- `--consume` — delete the input report files **after** a successful write. Never before, so a write error will not lose data.

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
