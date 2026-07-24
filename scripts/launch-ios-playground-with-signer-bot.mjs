#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const repoRoot = resolve(import.meta.dirname, "..");
loadDotEnv(resolve(repoRoot, ".env"));

const device =
  process.env.TRUAPI_IOS_E2E_DEVICE ??
  process.env.IOS_SIMULATOR_DEVICE ??
  "E606A6AE-0432-405F-A772-2A09515C896D";
const bundle =
  process.env.TRUAPI_IOS_E2E_BUNDLE ?? "io.pcf.polkadotapp.develop";
const app =
  process.env.TRUAPI_IOS_E2E_APP ??
  resolve(
    repoRoot,
    "hosts/ios/build/DerivedData/Build/Products/Debug-iphonesimulator/polkadot-app.app",
  );
const productUrl =
  process.env.TRUAPI_IOS_E2E_PRODUCT_URL ?? "http://localhost:3000";
const botBase = (
  process.env.SIGNER_BOT_BASE_URL ?? "https://signing-bot-dev.novasama-tech.org/"
).replace(/\/$/, "");
const botToken = process.env.SIGNER_BOT_SVC_TOKEN;
const botNetwork = process.env.SIGNER_BOT_NETWORK ?? "paseo-next-v2";

if (!botToken) {
  throw new Error("SIGNER_BOT_SVC_TOKEN is required in .env");
}
if (!existsSync(app)) {
  throw new Error(`iOS app bundle not found: ${app}`);
}

const user = await provisionSignerBotUser();
console.log(
  JSON.stringify({
    signerBotUser: user.username,
    liteUsername: user.liteUsername,
    network: user.network,
    attested: true,
    publicKeyHexPrefix: user.publicKeyHex?.slice(0, 10),
  }),
);

run("open", ["-a", "Simulator"], { stdio: "ignore" });
spawnSync("xcrun", ["simctl", "boot", device], {
  stdio: "ignore",
});
run("xcrun", ["simctl", "bootstatus", device, "-b"]);
spawnSync("xcrun", ["simctl", "terminate", device, bundle], {
  stdio: "ignore",
});
spawnSync("xcrun", ["simctl", "uninstall", device, bundle], {
  stdio: "ignore",
});
run("xcrun", ["simctl", "install", device, app]);
run("xcrun", ["simctl", "launch", "--terminate-running-process", device, bundle], {
  env: {
    ...process.env,
    SIMCTL_CHILD_RUST_BACKTRACE: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_OPEN_BROWSE: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_PRODUCT_URL: productUrl,
    SIMCTL_CHILD_TRUAPI_IOS_E2E_SIGNER_BOT_NETWORK: botNetwork,
    SIMCTL_CHILD_TRUAPI_IOS_E2E_SIGNER_BOT_MNEMONIC: user.mnemonic,
    SIMCTL_CHILD_TRUAPI_IOS_E2E_SIGNER_BOT_LITE_USERNAME: user.liteUsername,
  },
});

async function provisionSignerBotUser() {
  const requested = nonEmpty(process.env.TRUAPI_IOS_E2E_SIGNER_BOT_USERNAME);
  const username = requested ?? generateUsername();
  const existing = requested ? await getUser(username).catch(() => null) : null;
  const created =
    existing ??
    (await request("/api/users", {
      method: "POST",
      body: JSON.stringify({ username, network: botNetwork }),
    }));

  let liteUsername = nonEmpty(created.liteUsername);
  if (created.attested !== true || !liteUsername) {
    const attestation = await request(
      `/api/users/${encodeURIComponent(username)}/attest`,
      {
        method: "POST",
        body: "{}",
      },
    );
    liteUsername = nonEmpty(attestation.liteUsername) ?? liteUsername;
  }

  const user = await getUser(username);
  liteUsername = nonEmpty(user.liteUsername) ?? liteUsername;
  if (!nonEmpty(user.mnemonic)) {
    throw new Error("signer-bot user did not include mnemonic");
  }
  if (!liteUsername) {
    throw new Error("signer-bot user did not include a LitePeople username");
  }
  if (user.attested !== true) {
    throw new Error("signer-bot user is not attested");
  }

  return {
    username,
    network: user.network ?? botNetwork,
    mnemonic: user.mnemonic,
    liteUsername,
    publicKeyHex: user.publicKeyHex,
  };
}

async function getUser(username) {
  return await request(`/api/users/${encodeURIComponent(username)}`);
}

async function request(path, init = {}) {
  const response = await fetch(`${botBase}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${botToken}`,
      Accept: "application/json",
      "Content-Type": "application/json",
      ...(init.headers ?? {}),
    },
  });
  const text = await response.text();
  let body = null;
  if (text.length > 0) {
    try {
      body = JSON.parse(text);
    } catch {
      body = null;
    }
  }
  if (!response.ok) {
    throw new Error(
      `${init.method ?? "GET"} ${path} -> ${response.status} ${response.statusText}`,
    );
  }
  return body;
}

function generateUsername() {
  const alphabet = "abcdefghijklmnopqrstuvwxyz";
  let suffix = "";
  for (let index = 0; index < 6; index++) {
    suffix += alphabet[Math.floor(Math.random() * alphabet.length)];
  }
  return `iostests${suffix}`;
}

function loadDotEnv(path) {
  if (!existsSync(path)) {
    return;
  }
  for (const rawLine of readFileSync(path, "utf8").split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) {
      continue;
    }
    const eq = line.indexOf("=");
    if (eq <= 0) {
      continue;
    }
    const key = line.slice(0, eq).trim();
    if (process.env[key] !== undefined) {
      continue;
    }
    process.env[key] = parseDotEnvValue(line.slice(eq + 1).trim());
  }
}

function parseDotEnvValue(value) {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1);
  }
  return value;
}

function nonEmpty(value) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", ...options });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with ${result.status}`);
  }
}
