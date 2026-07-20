#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Keep release metadata derived from @parity/truapi's version in sync.
 *
 * Update Cargo.toml and the host package's dependency range:
 *   npm run sync-release-versions
 *
 * Verify those files and package-lock.json without writing changes:
 *   npm run check-release-versions
 *
 * `npm run version-packages` runs the update after consuming changesets and
 * then refreshes package-lock.json.
 */

const command = "sync-release-versions";
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const truapiPath = resolve(repoRoot, "js/packages/truapi/package.json");
const hostPath = resolve(repoRoot, "js/packages/truapi-host/package.json");
const cargoPath = resolve(repoRoot, "rust/crates/truapi/Cargo.toml");
const lockPath = resolve(repoRoot, "package-lock.json");
const check = process.argv.includes("--check");

const truapi = readJson(truapiPath);
const host = readJson(hostPath);
if (typeof truapi.version !== "string" || truapi.version.length === 0) {
  fail(`could not read .version from ${truapiPath}`);
}

const expectedDependency = `^${truapi.version}`;
const actualDependency = host.dependencies?.["@parity/truapi"];
const cargo = readFile(cargoPath);
const cargoVersionLine = /^version = "([^"]*)"$/m;
const cargoVersion = cargo.match(cargoVersionLine)?.[1];
if (cargoVersion === undefined) {
  fail(`could not find a top-level \`version = "..."\` line in ${cargoPath}`);
}

if (check) {
  const errors = [];
  if (cargoVersion !== truapi.version) {
    errors.push(
      `rust/crates/truapi/Cargo.toml is ${cargoVersion}; expected ${truapi.version}`,
    );
  }
  if (actualDependency !== expectedDependency) {
    errors.push(
      `js/packages/truapi-host/package.json requires ${actualDependency ?? "<missing>"}; expected ${expectedDependency}`,
    );
  }

  const lock = readJson(lockPath);
  const lockedTruapi = lock.packages?.["js/packages/truapi"]?.version;
  const lockedHost = lock.packages?.["js/packages/truapi-host"]?.version;
  const lockedDependency =
    lock.packages?.["js/packages/truapi-host"]?.dependencies?.[
      "@parity/truapi"
    ];
  if (lockedTruapi !== truapi.version) {
    errors.push(
      `package-lock.json records @parity/truapi ${lockedTruapi ?? "<missing>"}; expected ${truapi.version}`,
    );
  }
  if (lockedHost !== host.version) {
    errors.push(
      `package-lock.json records @parity/truapi-host ${lockedHost ?? "<missing>"}; expected ${host.version}`,
    );
  }
  if (lockedDependency !== expectedDependency) {
    errors.push(
      `package-lock.json records the host dependency as ${lockedDependency ?? "<missing>"}; expected ${expectedDependency}`,
    );
  }

  if (errors.length > 0) {
    for (const error of errors) console.error(`${command}: ${error}`);
    console.error(`${command}: run \`npm run version-packages\` to sync`);
    process.exit(1);
  }

  console.log(
    `${command}: Cargo.toml, host dependencies, and package-lock.json use @parity/truapi ${truapi.version}`,
  );
  process.exit(0);
}

const nextCargo = cargo.replace(
  cargoVersionLine,
  `version = "${truapi.version}"`,
);
if (nextCargo === cargo) {
  console.log(`${command}: Cargo.toml already uses ${truapi.version}`);
} else {
  writeFileSync(cargoPath, nextCargo);
  console.log(`${command}: updated Cargo.toml to ${truapi.version}`);
}

if (actualDependency === expectedDependency) {
  console.log(
    `${command}: host already requires @parity/truapi ${expectedDependency}`,
  );
} else {
  host.dependencies ??= {};
  host.dependencies["@parity/truapi"] = expectedDependency;
  writeFileSync(hostPath, `${JSON.stringify(host, null, 2)}\n`);
  console.log(
    `${command}: updated host dependency to @parity/truapi ${expectedDependency}`,
  );
}

function readJson(path) {
  return JSON.parse(readFile(path));
}

function readFile(path) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    console.error(`${command}: could not read ${path}`);
    console.error(error);
    process.exit(1);
  }
}

function fail(message) {
  console.error(`${command}: ${message}`);
  process.exit(1);
}
