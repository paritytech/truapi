# TrUAPI Explorer

Docs-only browser for the TrUAPI service surface. All trait and type data is sourced from the codegen registry exposed by `@parity/truapi/explorer/versions`. Built with Vite + React + Tailwind v4. Static SPA, dark-only.

## Host compatibility matrix

The **Compatibility** page (`/v/<version>/compatibility`) renders a host × method matrix aggregated from the playground's per-host Diagnosis reports. The committed per-host reports under [`diagnosis-reports/`](diagnosis-reports/) are the source of truth; [`src/data/compatibility.ts`](src/data/compatibility.ts) is a generated artifact (git-ignored) that [`scripts/aggregate-diagnosis-matrix.mjs`](scripts/aggregate-diagnosis-matrix.mjs) rebuilds from those reports at `dev` / `build` / `lint` time (via the `predev` / `prebuild` / `prelint` scripts). It is the **only** runtime-derived data in the explorer; everything else flows from Rust via codegen.

### Updating the matrix

Because the matrix is regenerated from `diagnosis-reports/` on every `dev` / `build` / `lint`, you only ever commit reports, never `src/data/compatibility.ts`.

**From the playground (recommended).** Open the playground in the host you want to (re)measure, run the Diagnosis, and click **Submit report ↗**. That files a pre-filled `diagnosis-report` issue; the [`diagnosis-report`](../.github/workflows/diagnosis-report.yml) workflow writes the report to `diagnosis-reports/<host>.md` and opens (or updates) that host's PR.

**By hand.** Click **Copy report** instead (see [`../playground/README.md#diagnosis`](../playground/README.md#diagnosis)), save the markdown to a host-named file (e.g. `web.md`, `desktop.md`, `android.md`, `ios.md`), drop it into [`diagnosis-reports/`](diagnosis-reports/) overwriting that host's previous report, and commit. Run `npm run generate-matrix` from `explorer/` to preview locally (or just `npm run dev`, which regenerates first). The Compatibility page and each method's Host support row pick up the new data on the next build / Vite HMR.

### Data shape

[`src/data/compatibility-types.ts`](src/data/compatibility-types.ts) holds the schema. Each method row carries one `pass | fail | null` entry per host column; `null` means the method was absent from (or skipped in) that host's report. Methods with no measurement on any host are dropped from the matrix. Columns are labelled by host mode (`Web` / `Desktop` / `Android` / `iOS`); when two reports share a mode, the filename disambiguates the label.

### Standalone CLI

The aggregator can be invoked directly:

```bash
node scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md          # markdown preview to stdout
node scripts/aggregate-diagnosis-matrix.mjs --explorer-out src/data/compatibility.ts diagnosis-reports
```

Flags:

- `--explorer-out <file>` — write the TypeScript matrix module instead of stdout markdown.
