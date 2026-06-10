#!/usr/bin/env node
// Rebuild the truapi-server WASM artefacts generated under
// `dist/wasm/{web,node}/`. wasm-pack is required.

import { execFile } from "node:child_process";
import { rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "../../..");
const rustCrate = resolve(repoRoot, "rust/crates/truapi-server");

function args(target, outDir) {
  return [
    "build",
    rustCrate,
    "--target",
    target,
    "--out-dir",
    outDir,
    "--out-name",
    "truapi_server",
    "--no-default-features",
  ];
}

async function build(target, subdir) {
  const outDir = resolve(pkgRoot, "dist/wasm", subdir);
  process.stdout.write(`wasm-pack build --target ${target} → ${outDir}\n`);
  try {
    await execFileAsync("wasm-pack", args(target, outDir), { cwd: repoRoot });
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
  // wasm-pack writes a nested `.gitignore: *`; the repo-level ignore already
  // owns generated WASM outputs.
  await rm(resolve(outDir, ".gitignore"), { force: true });
}

await build("web", "web");
await build("nodejs", "node");
