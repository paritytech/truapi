#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const truapiPath = resolve(repoRoot, "js/packages/truapi/package.json");
const hostPath = resolve(repoRoot, "js/packages/truapi-host/package.json");
const lockPath = resolve(repoRoot, "package-lock.json");
const check = process.argv.includes("--check");

const truapi = readJson(truapiPath);
const host = readJson(hostPath);
if (typeof truapi.version !== "string" || truapi.version.length === 0) {
  console.error(
    `sync-internal-dependencies: could not read .version from ${truapiPath}`,
  );
  process.exit(1);
}
const expected = `^${truapi.version}`;
const actual = host.dependencies?.["@parity/truapi"];

if (check) {
  const errors = [];
  if (actual !== expected) {
    errors.push(
      `js/packages/truapi-host/package.json requires ${actual ?? "<missing>"}; expected ${expected}`,
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
  if (lockedDependency !== expected) {
    errors.push(
      `package-lock.json records the host dependency as ${lockedDependency ?? "<missing>"}; expected ${expected}`,
    );
  }

  if (errors.length > 0) {
    for (const error of errors)
      console.error(`sync-internal-dependencies: ${error}`);
    console.error(
      "sync-internal-dependencies: run `npm run sync-internal-dependencies` followed by `npm install --package-lock-only --ignore-scripts`",
    );
    process.exit(1);
  }

  console.log(
    `sync-internal-dependencies: host and lockfile use @parity/truapi ${expected}`,
  );
  process.exit(0);
}

if (actual === expected) {
  console.log(
    `sync-internal-dependencies: host already uses @parity/truapi ${expected}`,
  );
  process.exit(0);
}

host.dependencies ??= {};
host.dependencies["@parity/truapi"] = expected;
writeFileSync(hostPath, `${JSON.stringify(host, null, 2)}\n`);
console.log(
  `sync-internal-dependencies: updated @parity/truapi-host to ${expected}`,
);

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    console.error(`sync-internal-dependencies: could not read ${path}`);
    console.error(error);
    process.exit(1);
  }
}
