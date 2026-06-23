#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pkgPath = resolve(repoRoot, "js/packages/truapi/package.json");
const cargoPath = resolve(repoRoot, "rust/crates/truapi/Cargo.toml");

const { version } = JSON.parse(readFileSync(pkgPath, "utf8"));
if (typeof version !== "string" || version.length === 0) {
  console.error(`sync-cargo-version: could not read .version from ${pkgPath}`);
  process.exit(1);
}

const cargo = readFileSync(cargoPath, "utf8");
const versionLine = /^version = "[^"]*"$/m;
if (!versionLine.test(cargo)) {
  console.error(
    `sync-cargo-version: could not find a top-level \`version = "…"\` line in ${cargoPath}`,
  );
  process.exit(1);
}

const next = cargo.replace(versionLine, `version = "${version}"`);
if (next === cargo) {
  console.log(`sync-cargo-version: already at ${version}`);
  process.exit(0);
}

writeFileSync(cargoPath, next);
console.log(`sync-cargo-version: bumped rust/crates/truapi/Cargo.toml to ${version}`);
