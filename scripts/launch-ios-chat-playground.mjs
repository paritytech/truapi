#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { dirname, extname, resolve, sep } from "node:path";
import {
  CHAT_DIAGNOSIS_HEADING,
  decodeTextMessage,
  labelChatDiagnosisReport,
} from "./lib/chat-diagnosis-report.mjs";
import {
  DEFAULT_BUNDLE,
  appGroupId,
  bootAndInstallApp,
  capture,
  defaultAppPath,
  delay,
  isLoopback,
  readPlistValue,
  run,
  runAsync,
  waitFor,
} from "./lib/ios-simulator.mjs";

const repoRoot = resolve(import.meta.dirname, "..");
const bundle = process.env.TRUAPI_IOS_E2E_BUNDLE ?? DEFAULT_BUNDLE;
const app = process.env.TRUAPI_IOS_E2E_APP ?? defaultAppPath(repoRoot);
const productRoot = resolve(
  repoRoot,
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_DIR ?? "playground",
);
const productHost =
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST ?? "truapi-playground.dot";
const productName =
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME ?? "TrUAPI Playground";
const roomId = process.env.TRUAPI_IOS_E2E_CHAT_ROOM_ID ?? "truapi-playground";
const expectDiagnosis = process.env.TRUAPI_IOS_E2E_CHAT_DIAGNOSIS !== "0";
const message =
  process.env.TRUAPI_IOS_E2E_CHAT_MESSAGE ??
  (expectDiagnosis ? "!diagnose" : "!echo hello");
const expectedReply =
  process.env.TRUAPI_IOS_E2E_CHAT_EXPECTED_REPLY ?? "Echo: hello";
const expectedStartupMessage =
  process.env.TRUAPI_IOS_E2E_CHAT_EXPECTED_STARTUP_MESSAGE ?? "";
const expectCustomRenderer =
  process.env.TRUAPI_IOS_E2E_CHAT_EXPECT_CUSTOM_RENDERER !== "0";
const worker = resolve(productRoot, "out/worker/index.js");
const productUrl =
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_URL ?? "http://127.0.0.1:3100";
const screenshot = resolve(
  repoRoot,
  process.env.TRUAPI_IOS_E2E_CHAT_SCREENSHOT ??
    "artifacts/truapi-playground-chat.png",
);
const reportPath = resolve(
  repoRoot,
  process.env.TRUAPI_IOS_E2E_CHAT_REPORT ??
    "playground/test-results/ios-chat/diagnosis-report.md",
);

if (!existsSync(app)) {
  throw new Error(`iOS app bundle not found: ${app}`);
}
if (!existsSync(resolve(productRoot, "package.json"))) {
  throw new Error(`Chat product source not found: ${productRoot}`);
}

const linkedTruapiRoot = process.env.TRUAPI_IOS_E2E_CHAT_TRUAPI_DIR;
if (linkedTruapiRoot) {
  const truapiRoot = resolve(repoRoot, linkedTruapiRoot);
  run("yarn", ["build"], { cwd: truapiRoot });
  run("yarn", ["link"], { cwd: truapiRoot });
  run("yarn", ["link", "@parity/truapi"], { cwd: productRoot });
}

// Build the product while the simulator boots; the output is first needed at
// the worker-copy step below.
const productBuild =
  process.env.TRUAPI_IOS_E2E_SKIP_PRODUCT_BUILD !== "1"
    ? runAsync("yarn", ["build"], { cwd: productRoot })
    : Promise.resolve();
productBuild.catch(() => {});

const device = bootAndInstallApp(app);

await productBuild;
if (!existsSync(worker)) {
  throw new Error(`Chat product worker not found after build: ${worker}`);
}

const appData = capture("xcrun", [
  "simctl",
  "get_app_container",
  device.udid,
  bundle,
  "data",
]).trim();
const connectionMarkers = [
  resolve(appData, "tmp/truapi-e2e", `connected-chat-${productHost}`),
];
const customRendererMarker = resolve(
  appData,
  "tmp/truapi-e2e/custom-renderer-update",
);
for (const marker of [...connectionMarkers, customRendererMarker]) {
  if (existsSync(marker)) {
    unlinkSync(marker);
  }
}
const workerDestination = resolve(
  appData,
  "Library/Application Support/Products",
  productHost,
  "ChatExtension/index.js",
);
const workerDestinations = [workerDestination];
const contentHashPreferences = resolve(
  appData,
  "Library/Preferences/io.products.dotns.cache.plist",
);
const currentCachedWorkerDestination = () => {
  // A missing key means this product has no cached DotNs content, so the
  // fallback destination is authoritative.
  const contentHash = readPlistValue(contentHashPreferences, productHost);
  if (contentHash && /^[0-9a-f]+$/i.test(contentHash)) {
    return resolve(
      appData,
      "Library/Application Support/DotNsContent",
      contentHash,
      "worker/index.js",
    );
  }
  return undefined;
};
const initialCachedWorkerDestination = currentCachedWorkerDestination();
if (initialCachedWorkerDestination) {
  workerDestinations.push(initialCachedWorkerDestination);
}
for (const destination of workerDestinations) {
  mkdirSync(resolve(destination, ".."), { recursive: true });
  cpSync(worker, destination);
}

const appGroup = capture("xcrun", [
  "simctl",
  "get_app_container",
  device.udid,
  bundle,
  appGroupId(bundle),
]).trim();
const userDataDatabase = resolve(appGroup, "CoreData/UserDataModel.sqlite");
const chatIdentifier = `1:${productHost}:${roomId}`;
const messageWatermark = existsSync(userDataDatabase)
  ? latestMessageId(userDataDatabase, chatIdentifier)
  : 0;

const productServer = await startProductServer(
  productUrl,
  resolve(productRoot, "out"),
);
try {
  const launchApp = () =>
    run(
      "xcrun",
      ["simctl", "launch", "--terminate-running-process", device.udid, bundle],
      {
        env: {
          ...process.env,
          SIMCTL_CHILD_RUST_BACKTRACE: "1",
          SIMCTL_CHILD_TRUAPI_IOS_E2E_BROWSE: "1",
          SIMCTL_CHILD_TRUAPI_IOS_E2E_PRODUCT_HOST: productHost,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_PRODUCT_URL: productUrl,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST: productHost,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME: productName,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_ROOM_ID: roomId,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_MESSAGE: message,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_OPEN_CHAT: "1",
          SIMCTL_CHILD_TRUAPI_IOS_E2E_RUNTIME_MARKERS: "1",
        },
      },
    );

  launchApp();

  await waitForFiles(
    connectionMarkers,
    60_000,
    "Ensure the selected simulator has completed Polkadot onboarding.",
  );

  const activeCachedWorkerDestination = currentCachedWorkerDestination();
  if (
    activeCachedWorkerDestination &&
    !filesHaveEqualContents(worker, activeCachedWorkerDestination)
  ) {
    mkdirSync(resolve(activeCachedWorkerDestination, ".."), { recursive: true });
    cpSync(worker, activeCachedWorkerDestination);
    if (!workerDestinations.includes(activeCachedWorkerDestination)) {
      workerDestinations.push(activeCachedWorkerDestination);
    }
    for (const marker of [...connectionMarkers, customRendererMarker]) {
      if (existsSync(marker)) unlinkSync(marker);
    }
    launchApp();
    await waitForFiles(
      connectionMarkers,
      60_000,
      "Ensure the selected simulator has completed Polkadot onboarding.",
    );
  }

  if (expectDiagnosis) {
    const report = await waitForTextPrefix(
      userDataDatabase,
      chatIdentifier,
      messageWatermark,
      CHAT_DIAGNOSIS_HEADING,
    );
    const hostReport = labelChatDiagnosisReport(report, "iOS");
    mkdirSync(dirname(reportPath), { recursive: true });
    writeFileSync(reportPath, `${hostReport}\n`);
  } else {
    if (expectedStartupMessage) {
      await waitForTextPrefix(
        userDataDatabase,
        chatIdentifier,
        messageWatermark,
        expectedStartupMessage,
      );
    }
    await waitForTextPrefix(
      userDataDatabase,
      chatIdentifier,
      messageWatermark,
      expectedReply,
    );
  }
  if (expectCustomRenderer) {
    await waitForFiles([customRendererMarker], 30_000);
  }
  await delay(2_000);
  mkdirSync(dirname(screenshot), { recursive: true });
  run("xcrun", ["simctl", "io", device.udid, "screenshot", screenshot]);
} finally {
  productServer?.close();
}

console.log(
  JSON.stringify({
    device: device.name,
    deviceId: device.udid,
    app,
    bundle,
    productHost,
    productName,
    roomId,
    message,
    diagnosisVerified: expectDiagnosis,
    customRendererVerified: expectCustomRenderer,
    productUrl,
    verifiedExecutions: ["Chat"],
    worker,
    workerDestination,
    workerDestinations,
    report: expectDiagnosis ? reportPath : undefined,
    screenshot,
    verified: true,
  }),
);

async function startProductServer(urlString, root) {
  const url = new URL(urlString);
  if (!isLoopback(url)) {
    throw new Error(
      `Product URL must be loopback for this E2E test: ${urlString}`,
    );
  }

  try {
    const response = await fetch(urlString);
    if (response.ok && (await response.text()).includes(productName)) {
      return null;
    }
    throw new Error(`${urlString} is serving a different application`);
  } catch (error) {
    if (
      error instanceof Error &&
      error.message.includes("different application")
    ) {
      throw error;
    }
  }

  const server = createServer((request, response) => {
    try {
      const pathname = decodeURIComponent(
        new URL(request.url ?? "/", urlString).pathname,
      );
      let file = resolve(root, `.${pathname}`);
      if (file !== root && !file.startsWith(`${root}${sep}`)) {
        response.writeHead(403).end();
        return;
      }
      if (statSync(file).isDirectory()) {
        file = resolve(file, "index.html");
      }
      response.setHeader("Content-Type", contentType(file));
      const content = readFileSync(file);
      response.end(content);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(Number(url.port || 80), url.hostname, resolveListen);
  });
  return server;
}

function contentType(file) {
  switch (extname(file)) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
      return "text/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".json":
      return "application/json";
    case ".png":
      return "image/png";
    case ".svg":
      return "image/svg+xml";
    default:
      return "application/octet-stream";
  }
}

function waitForFiles(files, timeoutMs, hint) {
  return waitFor(() => files.every(existsSync), {
    timeoutMs,
    message: () =>
      `Timed out waiting for files: ${files.join(", ")}${hint ? `\n${hint}` : ""}`,
  });
}

function filesHaveEqualContents(first, second) {
  return (
    existsSync(first) &&
    existsSync(second) &&
    readFileSync(first).equals(readFileSync(second))
  );
}

function latestMessageId(database, identifier) {
  const query = `
    SELECT COALESCE(MAX(message.Z_PK), 0)
    FROM ZCDCHATMESSAGE AS message
    JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
    WHERE chat.ZIDENTIFIER = ${sqlString(identifier)};
  `;
  const value = capture("sqlite3", [database, query]).trim();
  return Number.parseInt(value, 10) || 0;
}

function waitForTextPrefix(database, identifier, afterMessageId, prefix) {
  return waitFor(
    () => {
      if (!existsSync(database)) {
        return undefined;
      }
      const query = `
        SELECT hex(content.ZDATA)
        FROM ZCDCHATMESSAGE AS message
        JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
        JOIN ZCDMESSAGECONTENT AS content ON content.Z_PK = message.ZCONTENT
        WHERE chat.ZIDENTIFIER = ${sqlString(identifier)}
          AND message.Z_PK > ${afterMessageId}
        ORDER BY message.Z_PK;
      `;
      const values = capture("sqlite3", [database, query])
        .trim()
        .split(/\r?\n/)
        .filter(Boolean);
      for (const value of values) {
        const text = decodeTextMessage(value);
        if (text?.startsWith(prefix)) {
          return text;
        }
      }
      return undefined;
    },
    {
      timeoutMs: 30_000,
      message: () =>
        `Timed out waiting for a message starting with ${JSON.stringify(prefix)} in ${identifier}`,
    },
  );
}

function sqlString(value) {
  return `'${value.replaceAll("'", "''")}'`;
}
