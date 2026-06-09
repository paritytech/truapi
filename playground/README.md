# TrUAPI Playground

_Browse, edit, and call every TrUAPI method live against a connected Polkadot host._

The playground is an interactive reference for the TrUAPI: every method grouped by domain, with live request payload editing, one-click calls, and live subscriptions. It must be opened from inside a TrUAPI host so it can talk to the host over the wire.

**Live app:** [https://truapi-playground.dot.li/](https://truapi-playground.dot.li/)

## Features

- **Full method browser**: every TrUAPI service and method, each with a description and a Request / Response or Subscription badge.
- **Live calls**: edit a JSON request payload and fire the call against the connected host.
- **Subscriptions**: open and close streaming methods and watch events arrive in real time.
- **Auto-test view**: runs every method and reports pass / fail in one pass.
- **Diagnosis view**: runs the full surface and produces a copy-pasteable markdown report per host. The explorer's Compatibility page aggregates those into a cross-host matrix. See [Diagnosis](#diagnosis).
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

### Example conventions

An example **passes** when its promise resolves and **fails** when it throws. Use the ambient `assert(condition, ...message)` (no import) to fail explicitly — `assert(false, ...)` throws. `console.*` is pure output. For a `Result`, write `assert(r.isOk(), "<step> failed:", r)` (narrows `r` to `Ok`, includes the result in the failure message). Await subscriptions with `firstValueFrom(from(<observable>))`.

## Diagnosis

The Diagnosis view exercises every TrUAPI method against the connected host and emits a per-host pass/fail report you can copy out. Per-host reports feed the explorer's **Compatibility** page, which renders the host × method matrix; aggregation lives in the explorer (see [`explorer/README.md`](../explorer/README.md#host-compatibility-matrix)).

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
5. Click **Submit report ↗** to file a pre-filled GitHub issue that the `diagnosis-report` workflow turns into a per-host PR under `explorer/diagnosis-reports/`. (Or click **Copy report**, save the markdown to a host-named file like `web.md`, and update the matrix by hand — see [`../explorer/README.md`](../explorer/README.md#updating-the-matrix).)

The report looks like this:

```markdown
## Truapi Web Diagnosis

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
