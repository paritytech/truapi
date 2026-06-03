#!/usr/bin/env node
// Aggregate per-host TrUAPI diagnosis reports into the explorer's committed
// host × method compatibility matrix (columns = hosts, rows = methods),
// MDN browser-compat style.
//
// Each input is a diagnosis report as produced by the playground's "Copy
// report" button:
//
//   ## Truapi Desktop Diagnosis
//   _Generated: 2026-05-27T18:00:32.854Z_
//
//   | Method | Status | Details |
//   | --- | --- | --- |
//   | `Account/get_account` | ✅ |  |
//   | `Chat/post_message` | ❌ | host error: not implemented |
//   ...
//
// Keep one report per host you want to (re)measure (a host-named `*.md` file,
// e.g. `web.md`) in the explorer's `diagnosis-reports/` directory and run from
// `explorer/`:
//
//   npm run generate-matrix
//
// That rebuilds `src/data/compatibility.ts` from the reports in
// `diagnosis-reports/` — one column per report. The reports are committed too,
// so re-running a host overwrites its file and both the report diff and the
// regenerated matrix show what changed.
//
// Direct invocation also works for ad-hoc use:
//   node scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md           # markdown to stdout
//   node scripts/aggregate-diagnosis-matrix.mjs --explorer-out src/data/compatibility.ts diagnosis-reports
//
// Flags:
//   --explorer-out <file>   write a TypeScript module exporting `compatibility`
//
// The host column label is the mode from each report's title (Web / Desktop /
// Android / iOS / Unknown). Reports that share a mode are disambiguated with
// their filename. A method missing from a report renders as "—" in the markdown
// view and `null` in the TypeScript module.

import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, extname, join } from "node:path";

const TITLE_RE = /^##\s+Truapi\s+(.+?)\s+Diagnosis\s*$/im;
const GENERATED_RE = /^_?Generated:\s*(.+?)_?\s*$/m;
// A method row: `| Service/method | ✅ | optional details |`. The method cell
// may or may not be wrapped in backticks and the columns may be space-padded
// (markdown formatters do both), so backticks are optional here. Group 2 is the
// status icon; group 3 is the optional Details cell (captured up to the
// trailing pipe so an escaped `\|` inside it survives). Header (`| Method | … `)
// and separator (`| --- | … `) rows are dropped in parseReport: their first
// cell has no `/`.
const ROW_RE = /^\|\s*`?([^|`]+?)`?\s*\|\s*([^|]*?)\s*\|\s*(?:(.*?)\s*\|\s*)?$/;

function collectFiles(args) {
  const files = [];
  for (const arg of args) {
    if (statSync(arg).isDirectory()) {
      for (const name of readdirSync(arg).sort()) {
        if (extname(name) === ".md") files.push(join(arg, name));
      }
    } else {
      files.push(arg);
    }
  }
  return files;
}

function parseReport(file) {
  const text = readFileSync(file, "utf8");
  const titleMatch = text.match(TITLE_RE);
  const mode = titleMatch ? titleMatch[1].trim() : "Unknown";
  const reportedAtMatch = text.match(GENERATED_RE);
  const reportedAt = reportedAtMatch ? reportedAtMatch[1].trim() : "";
  const statuses = new Map();
  const details = new Map();
  const order = [];
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(ROW_RE);
    if (!m) continue;
    const method = m[1].trim();
    if (!method.includes("/")) continue;
    if (!statuses.has(method)) order.push(method);
    statuses.set(method, m[2].trim());
    const detail = (m[3] ?? "").trim();
    if (detail) details.set(method, detail.replace(/\\\|/g, "|"));
  }
  return { file, mode, reportedAt, statuses, details, order };
}

function columnLabels(reports) {
  const modeCounts = new Map();
  for (const r of reports) {
    modeCounts.set(r.mode, (modeCounts.get(r.mode) ?? 0) + 1);
  }
  return reports.map((r) => {
    if (modeCounts.get(r.mode) > 1) {
      return `${r.mode} (${basename(r.file, extname(r.file))})`;
    }
    return r.mode;
  });
}

function unionMethodOrder(reports) {
  const seen = new Set();
  const order = [];
  for (const r of reports) {
    for (const method of r.order) {
      if (!seen.has(method)) {
        seen.add(method);
        order.push(method);
      }
    }
  }
  return order;
}

// Map the icon-only status cell from a report to the typed enum used by the
// explorer. Only the pass/fail markers are real measurements; a skipped (⏭),
// idle, or running cell means "not measured", same as an absent row → null.
function statusOf(cell) {
  if (cell.startsWith("✅")) return "pass";
  if (cell.startsWith("❌")) return "fail";
  return null;
}

const KNOWN_MODES = new Set(["Web", "Desktop", "Android", "iOS"]);
// Canonical column order: desktop-class hosts first, then mobile, then unknown.
const MODE_ORDER = ["Desktop", "Web", "Android", "iOS", "Unknown"];

// The matrix schema only admits a fixed set of host modes; anything else
// (including a report with no recognizable title) collapses to "Unknown".
function normalizeMode(mode) {
  return KNOWN_MODES.has(mode) ? mode : "Unknown";
}

// The status of one method for one report's host, mapped to the typed enum, or
// null when the report doesn't mention the method at all.
function cellStatus(report, id) {
  const cell = report.statuses.get(id);
  return cell == null ? null : statusOf(cell);
}

// The failure detail for one method in one report, or undefined when none.
function cellDetail(report, id) {
  return report.details.get(id);
}

// Build the matrix from the parsed reports alone — one column per report, in
// canonical host order, with one row per method seen across the reports.
function buildMatrix(reports, labels, methods, generatedAt) {
  const reportByLabel = new Map(labels.map((label, i) => [label, reports[i]]));

  const hosts = reports
    .map((report, i) => ({
      label: labels[i],
      mode: normalizeMode(report.mode),
      reportedAt: report.reportedAt,
    }))
    .sort((a, b) => MODE_ORDER.indexOf(a.mode) - MODE_ORDER.indexOf(b.mode));

  const rows = methods.map((id) => {
    const results = {};
    const details = {};
    for (const host of hosts) {
      const report = reportByLabel.get(host.label);
      results[host.label] = cellStatus(report, id);
      const detail = cellDetail(report, id);
      if (detail) details[host.label] = detail;
    }
    const row = { id, results };
    if (Object.keys(details).length > 0) row.details = details;
    return row;
  });

  // Drop methods with no real measurement on any host (skipped everywhere) so
  // the matrix carries only what was actually exercised.
  const measured = rows.filter((row) =>
    Object.values(row.results).some((v) => v !== null),
  );

  return { generatedAt, hosts, methods: measured };
}

function renderMarkdown(reports, labels, methods) {
  const lines = [];
  lines.push("# TrUAPI Host Compatibility Matrix");
  lines.push(
    `_Generated: ${new Date().toISOString()} — aggregated from ${reports.length} report(s)_`,
  );
  lines.push("");
  lines.push(`| Method | ${labels.join(" | ")} |`);
  lines.push(`| --- | ${labels.map(() => "---").join(" | ")} |`);
  for (const method of methods) {
    const cells = reports.map((r) => r.statuses.get(method) ?? "—");
    lines.push(`| \`${method}\` | ${cells.join(" | ")} |`);
  }
  return lines.join("\n") + "\n";
}

function renderTypeScript(matrix) {
  return [
    "// AUTO-GENERATED by explorer/scripts/aggregate-diagnosis-matrix.mjs.",
    "// Source: per-host diagnosis reports run from the playground's Diagnosis",
    "// screen. Do not edit by hand — rerun `npm run generate-matrix` instead.",
    "",
    'import type { CompatibilityMatrix } from "./compatibility-types";',
    "",
    `export const compatibility: CompatibilityMatrix = ${JSON.stringify(matrix, null, 2)};`,
    "",
  ].join("\n");
}

function parseArgs(argv) {
  const paths = [];
  let explorerOut = null;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--explorer-out") {
      explorerOut = argv[++i];
    } else {
      paths.push(arg);
    }
  }
  return { paths, explorerOut };
}

function main() {
  const { paths, explorerOut } = parseArgs(process.argv.slice(2));
  if (paths.length === 0) {
    console.error(
      "usage: aggregate-diagnosis-matrix.mjs [--explorer-out <file>] <report.md|dir> [more...]",
    );
    process.exit(1);
  }

  const files = collectFiles(paths);
  if (files.length === 0) {
    console.error("no report files found");
    process.exit(1);
  }

  const reports = files.map(parseReport);
  const labels = columnLabels(reports);
  const methods = unionMethodOrder(reports);
  const generatedAt = new Date().toISOString();

  if (explorerOut) {
    const matrix = buildMatrix(reports, labels, methods, generatedAt);
    writeFileSync(explorerOut, renderTypeScript(matrix));
    console.error(`Wrote ${explorerOut} from ${reports.length} report(s).`);
  } else {
    process.stdout.write(renderMarkdown(reports, labels, methods));
  }
}

main();
