# TrUAPI Explorer

Docs-only browser for the TrUAPI service surface. All trait and type data is sourced from the codegen registry exposed by `@parity/truapi/explorer/versions`. Built with Vite + React + Tailwind v4. Static SPA, dark-only.

## Host compatibility matrix

The **Compatibility** page (`/v/<version>/compatibility`) renders a host × method matrix aggregated from the playground's per-host Diagnosis reports. The matrix data is committed at [`src/data/compatibility.ts`](src/data/compatibility.ts) — a typed module emitted by [`scripts/aggregate-diagnosis-matrix.mjs`](scripts/aggregate-diagnosis-matrix.mjs). It is the **only** runtime-derived data in the explorer; everything else flows from Rust via codegen.

### Updating the matrix

1. **Collect reports.** For each host you want to (re)measure, open the playground in that host, run the Diagnosis, and click **Copy report** (see [`../playground/README.md#diagnosis`](../playground/README.md#diagnosis)). Save each report to a host-named markdown file (e.g. `web.md`, `desktop.md`, `android.md`, `ios.md`).
2. **Drop them in.** Place each host-named `*.md` into [`diagnosis-reports/`](diagnosis-reports/), overwriting that host's previous report.
3. **Regenerate.** From the `explorer/` directory:

   ```bash
   npm run generate-matrix
   ```

   That **merges** the reports into `src/data/compatibility.ts`, leaving the inputs in `diagnosis-reports/`. Merging is per host: a report overwrites the column whose label matches it, a report for a new label adds a column, and host columns with no report this run are left as they were. So you can refresh just Desktop without re-running Web, or add Android / iOS columns one at a time. The Compatibility page (and each method's Host support row) picks up the new data on the next build / Vite HMR.
4. **Commit** the updated `src/data/compatibility.ts` together with the reports under `diagnosis-reports/`. Keeping the raw per-host reports in version control makes each run diffable against the last.

To rebuild the matrix from scratch — dropping every host column not present in the current run — add `--replace`:

```bash
npm run generate-matrix -- --replace
```

### Data shape

[`src/data/compatibility-types.ts`](src/data/compatibility-types.ts) holds the schema. Each method row carries one `pass | fail | null` entry per host column; `null` means the method was absent from that host's report (typically a method added after the report was taken, or a host that hasn't been re-measured since the method landed). Columns are labelled by host mode (`Web` / `Desktop` / `Android` / `iOS`); when two reports share a mode, the filename disambiguates the label.

### Standalone CLI

The aggregator can be invoked directly:

```bash
node scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md          # markdown preview to stdout
node scripts/aggregate-diagnosis-matrix.mjs --explorer-out src/data/compatibility.ts diagnosis-reports
```

Flags:

- `--explorer-out <file>` — write the TypeScript matrix module instead of stdout markdown. Merges into the file's existing matrix unless `--replace` is given.
- `--replace` — rebuild from the reports alone, dropping host columns not present in this run (default: merge).
