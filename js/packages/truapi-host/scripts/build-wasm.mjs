#!/usr/bin/env node
// Rebuild the browser truapi-server WASM artefacts generated under
// `dist/wasm/web/`. wasm-pack is required.

import { execFile } from "node:child_process";
import { readFile, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import {
  brotliCompress,
  brotliDecompress,
  constants as zlibConstants,
  gzip,
  gunzip,
} from "node:zlib";

const execFileAsync = promisify(execFile);
const brotliCompressAsync = promisify(brotliCompress);
const brotliDecompressAsync = promisify(brotliDecompress);
const gzipAsync = promisify(gzip);
const gunzipAsync = promisify(gunzip);
const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(__dirname, "..");
const repoRoot = resolve(pkgRoot, "../../..");
const rustCrate = resolve(repoRoot, "rust/crates/truapi-server");
const wasmProfile = process.env.TRUAPI_WASM_PROFILE ?? "release";
const wasmFileName = "truapi_server_bg.wasm";

function args(target, outDir) {
  const command = [
    "build",
    "--target",
    target,
    "--out-dir",
    outDir,
    "--out-name",
    "truapi_server",
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
  command.push(rustCrate, "--no-default-features");
  return command;
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  return `${(kib / 1024).toFixed(2)} MiB`;
}

function readVarUint(bytes, cursor) {
  let result = 0;
  let shift = 0;
  let position = cursor;
  while (position < bytes.length) {
    const byte = bytes[position];
    result += (byte & 0x7f) * 2 ** shift;
    position += 1;
    if ((byte & 0x80) === 0) {
      return [result, position];
    }
    shift += 7;
  }
  throw new Error("unterminated wasm varuint");
}

function readCustomSectionNames(bytes) {
  if (
    bytes.length < 8 ||
    bytes[0] !== 0x00 ||
    bytes[1] !== 0x61 ||
    bytes[2] !== 0x73 ||
    bytes[3] !== 0x6d
  ) {
    throw new Error("generated file is not a wasm module");
  }

  const names = [];
  let offset = 8;
  while (offset < bytes.length) {
    const sectionId = bytes[offset];
    offset += 1;
    const [sectionSize, payloadStart] = readVarUint(bytes, offset);
    const payloadEnd = payloadStart + sectionSize;
    if (payloadEnd > bytes.length) {
      throw new Error("wasm section extends past end of file");
    }
    if (sectionId === 0) {
      const [nameLength, nameStart] = readVarUint(bytes, payloadStart);
      const nameEnd = nameStart + nameLength;
      if (nameEnd > payloadEnd) {
        throw new Error("wasm custom section name extends past section end");
      }
      names.push(
        Buffer.from(bytes.subarray(nameStart, nameEnd)).toString("utf8"),
      );
    }
    offset = payloadEnd;
  }
  return names;
}

async function validateReleaseWasm(wasmPath) {
  if (wasmProfile !== "release") return;

  const wasm = await readFile(wasmPath);
  const customSections = readCustomSectionNames(wasm);
  const forbidden = customSections.filter(
    (name) =>
      name === "name" || name === "producers" || name.startsWith(".debug"),
  );
  if (forbidden.length > 0) {
    throw new Error(
      `release wasm retained debug/metadata custom sections: ${forbidden.join(", ")}`,
    );
  }
}

async function writeCompressedSidecars(wasmPath) {
  if (wasmProfile !== "release") return;

  const wasm = await readFile(wasmPath);
  const gzipBytes = await gzipAsync(wasm, { level: 9 });
  const brotliBytes = await brotliCompressAsync(wasm, {
    params: {
      [zlibConstants.BROTLI_PARAM_QUALITY]: 11,
    },
  });

  await writeFile(`${wasmPath}.gz`, gzipBytes);
  await writeFile(`${wasmPath}.br`, brotliBytes);

  const [gzipRoundTrip, brotliRoundTrip] = await Promise.all([
    gunzipAsync(gzipBytes),
    brotliDecompressAsync(brotliBytes),
  ]);
  if (!gzipRoundTrip.equals(wasm) || !brotliRoundTrip.equals(wasm)) {
    throw new Error("compressed wasm sidecar round-trip validation failed");
  }

  process.stdout.write(
    [
      `wasm size: ${formatBytes(wasm.length)}`,
      `gzip: ${formatBytes(gzipBytes.length)}`,
      `brotli: ${formatBytes(brotliBytes.length)}`,
    ].join(" | ") + "\n",
  );
}

async function build(target, subdir) {
  const outDir = resolve(pkgRoot, "dist/wasm", subdir);
  process.stdout.write(
    `wasm-pack build --target ${target} --${wasmProfile} → ${outDir}\n`,
  );
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
  const wasmPath = resolve(outDir, wasmFileName);
  await Promise.all([
    rm(`${wasmPath}.br`, { force: true }),
    rm(`${wasmPath}.gz`, { force: true }),
  ]);
  await validateReleaseWasm(wasmPath);
  await writeCompressedSidecars(wasmPath);
}

await build("web", "web");
