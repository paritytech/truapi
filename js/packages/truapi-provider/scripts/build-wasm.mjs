#!/usr/bin/env node
// Rebuild the browser `@parity/truapi-provider` WASM bundle under `dist/` from
// the `truapi-provider` Rust crate. wasm-pack is required.
//
// The `js` feature exposes the wasm-bindgen provider API and `networks` bundles
// the smoldot light client plus the chain-spec catalog, so a consumer can
// `connect(genesisHash)` against a bundled network without shipping its own
// specs.

import { execFile } from "node:child_process";
import { readFile, rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "../../..");
const rustCrate = resolve(repoRoot, "rust/crates/truapi-provider");
const outDir = resolve(pkgRoot, "dist");
const wasmProfile = process.env.TRUAPI_WASM_PROFILE ?? "release";

function args() {
  const command = [
    "build",
    "--target",
    "web",
    "--out-dir",
    outDir,
    "--out-name",
    "truapi_provider",
  ];
  if (wasmProfile === "dev") {
    command.push("--dev");
  } else if (wasmProfile === "profiling") {
    command.push("--profiling");
  } else if (wasmProfile !== "release") {
    throw new Error(
      `Unsupported TRUAPI_WASM_PROFILE=${wasmProfile}; expected release, dev, or profiling`,
    );
  }
  command.push(rustCrate, "--features", "js networks");
  return command;
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  return kib < 1024 ? `${kib.toFixed(1)} KiB` : `${(kib / 1024).toFixed(2)} MiB`;
}

process.stdout.write(
  `wasm-pack build --target web --${wasmProfile} → ${outDir}\n`,
);
try {
  await execFileAsync("wasm-pack", args(), { cwd: repoRoot });
} catch (err) {
  if (err?.code === "ENOENT") {
    console.error(
      "wasm-pack is required. Install it with `cargo install wasm-pack` " +
        "or see https://rustwasm.github.io/wasm-pack/installer/",
    );
    process.exit(1);
  }
  throw err;
}

// wasm-pack writes a nested `.gitignore: *`, its own minimal package.json, and
// copies the crate's license files into the bundle. This package's own
// package.json and top-level license files are authoritative, and the repo owns
// the ignore rules, so drop the generated copies.
await Promise.all([
  rm(resolve(outDir, ".gitignore"), { force: true }),
  rm(resolve(outDir, "package.json"), { force: true }),
  rm(resolve(outDir, "LICENSE-APACHE"), { force: true }),
]);

const wasm = await readFile(resolve(outDir, "truapi_provider_bg.wasm"));
process.stdout.write(`wasm size: ${formatBytes(wasm.length)}\n`);
