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
// That merges every report in `diagnosis-reports/` into the committed
// `src/data/compatibility.ts` source-of-truth. The reports are committed too,
// so re-running a host overwrites its file and the diff shows what changed.
// Merging is per host: a report whose column label matches an existing host
// overwrites that host's column, a report for a new label adds a column, and
// hosts with no report this run are left untouched. So you can refresh just the
// Desktop column without re-running Web, and add Android / iOS columns
// incrementally.
//
// Direct invocation also works for ad-hoc use:
//   node scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md           # markdown to stdout
//   node scripts/aggregate-diagnosis-matrix.mjs --explorer-out src/data/compatibility.ts diagnosis-reports
//
// Flags:
//   --explorer-out <file>   write a TypeScript module exporting `compatibility`
//   --replace               rebuild from the reports alone, dropping any host
//                           columns not present in this run (default: merge)
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
// explorer. Anything that doesn't start with the pass marker is treated as a
// failure.
function statusOf(cell) {
  if (cell.startsWith("✅")) return "pass";
  return "fail";
}

const KNOWN_MODES = new Set(["Web", "Desktop", "Android", "iOS"]);

// The matrix schema only admits a fixed set of host modes; anything else
// (including a report with no recognizable title) collapses to "Unknown".
function normalizeMode(mode) {
  return KNOWN_MODES.has(mode) ? mode : "Unknown";
}

// Read the `compatibility` object back out of a previously generated TypeScript
// module so a new run can merge into it. Returns null when the file is absent
// or unparseable (e.g. first-ever run), in which case we start from scratch.
function readExistingMatrix(file) {
  let text;
  try {
    text = readFileSync(file, "utf8");
  } catch {
    return null;
  }
  const match = text.match(/compatibility[^=]*=\s*(\{[\s\S]*\})\s*;/);
  if (!match) return null;
  try {
    return JSON.parse(match[1]);
  } catch {
    return null;
  }
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

// Merge freshly parsed reports into the prior matrix. Each report upserts its
// own column (matched by label); columns with no report this run keep their
// previous values. A null `prior` (e.g. first run, or `--replace`) means only
// the reports in this run survive. `methods` is the union method order from the
// reports; existing method rows are preserved and extended.
function buildMatrix(prior, reports, labels, methods, generatedAt) {
  const reportByLabel = new Map(labels.map((label, i) => [label, reports[i]]));

  const hosts = (prior?.hosts ?? []).map((h) => ({ ...h }));
  for (let i = 0; i < reports.length; i++) {
    const host = {
      label: labels[i],
      mode: normalizeMode(reports[i].mode),
      reportedAt: reports[i].reportedAt,
    };
    const at = hosts.findIndex((h) => h.label === host.label);
    if (at >= 0) hosts[at] = host;
    else hosts.push(host);
  }

  const order = [];
  const seen = new Set();
  for (const id of [...(prior?.methods ?? []).map((m) => m.id), ...methods]) {
    if (!seen.has(id)) {
      seen.add(id);
      order.push(id);
    }
  }

  const priorById = new Map((prior?.methods ?? []).map((m) => [m.id, m]));
  const rows = order.map((id) => {
    const results = {};
    const details = {};
    for (const host of hosts) {
      const report = reportByLabel.get(host.label);
      if (report) {
        results[host.label] = cellStatus(report, id);
        const detail = cellDetail(report, id);
        if (detail) details[host.label] = detail;
      } else {
        const prevRow = priorById.get(id);
        results[host.label] = prevRow?.results?.[host.label] ?? null;
        const prevDetail = prevRow?.details?.[host.label];
        if (prevDetail) details[host.label] = prevDetail;
      }
    }
    const row = { id, results };
    if (Object.keys(details).length > 0) row.details = details;
    return row;
  });

  return { generatedAt, hosts, methods: rows };
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
  let replace = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--explorer-out") {
      explorerOut = argv[++i];
    } else if (arg === "--replace") {
      replace = true;
    } else {
      paths.push(arg);
    }
  }
  return { paths, explorerOut, replace };
}

function main() {
  const { paths, explorerOut, replace } = parseArgs(process.argv.slice(2));
  if (paths.length === 0) {
    console.error(
      "usage: aggregate-diagnosis-matrix.mjs [--explorer-out <file>] [--replace] <report.md|dir> [more...]",
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

  let merged = false;
  if (explorerOut) {
    const existing = replace ? null : readExistingMatrix(explorerOut);
    merged = existing != null;
    const matrix = buildMatrix(existing, reports, labels, methods, generatedAt);
    writeFileSync(explorerOut, renderTypeScript(matrix));
  } else {
    process.stdout.write(renderMarkdown(reports, labels, methods));
  }

  if (explorerOut) {
    console.error(
      `Wrote ${explorerOut} from ${reports.length} report(s)${
        replace ? " (replaced)" : merged ? " (merged)" : ""
      }.`,
    );
  }
}

main();
