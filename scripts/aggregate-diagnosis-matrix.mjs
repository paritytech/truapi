#!/usr/bin/env node
// Aggregate multiple TrUAPI playground diagnosis reports into a single host ×
// method compatibility matrix (columns = hosts, rows = methods), MDN
// browser-compat style.
//
// Each input is a diagnosis report as produced by the playground's "Copy
// report" button:
//
//   ## Truapi Desktop Diagnosis
//   _Generated: 2026-05-27T18:00:32.854Z_
//
//   | Method | Status |
//   | --- | --- |
//   | `Account/get_account` | ✅ |
//   ...
//
// Usage:
//   node scripts/aggregate-diagnosis-matrix.mjs web.md desktop.md > matrix.md
//   node scripts/aggregate-diagnosis-matrix.mjs reports/        # all *.md in dir
//   node scripts/aggregate-diagnosis-matrix.mjs --out matrix.md --consume pending-reports
//
// Flags:
//   --out <file>   write the matrix to <file> instead of stdout
//   --consume      delete the input report files after a successful write
//
// The host column label is the mode from each report's title (Web / Desktop /
// Unknown). Reports that share a mode are disambiguated with their filename. A
// method missing from a report renders as "—".

import {
  readFileSync,
  readdirSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { basename, extname, join } from "node:path";

const TITLE_RE = /^##\s+Truapi\s+(.+?)\s+Diagnosis\s*$/im;
// | `Service/method` | ✅ pass |   (the header row's "Method" cell has no
// backticks, so it is skipped automatically)
const ROW_RE = /^\|\s*`([^`]+)`\s*\|\s*(.+?)\s*\|\s*$/;

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
  const statuses = new Map();
  const order = [];
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(ROW_RE);
    if (!m) continue;
    const method = m[1].trim();
    if (!statuses.has(method)) order.push(method);
    statuses.set(method, m[2].trim());
  }
  return { file, mode, statuses, order };
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

function parseArgs(argv) {
  const paths = [];
  let out = null;
  let consume = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--out") {
      out = argv[++i];
    } else if (arg === "--consume") {
      consume = true;
    } else {
      paths.push(arg);
    }
  }
  return { paths, out, consume };
}

function main() {
  const { paths, out, consume } = parseArgs(process.argv.slice(2));
  if (paths.length === 0) {
    console.error(
      "usage: aggregate-diagnosis-matrix.mjs [--out <file>] [--consume] <report.md|dir> [more...]",
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
  const matrix = lines.join("\n") + "\n";

  if (out) {
    writeFileSync(out, matrix);
  } else {
    process.stdout.write(matrix);
  }

  // Delete inputs only after the matrix is safely written.
  if (consume) {
    for (const file of files) unlinkSync(file);
  }

  if (out) {
    console.error(
      `Wrote ${out} from ${reports.length} report(s)${consume ? " (consumed)" : ""}.`,
    );
  }
}

main();
