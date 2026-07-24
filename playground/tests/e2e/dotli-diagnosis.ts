// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import {
  chromium,
  type BrowserContext,
  type Frame,
  type Page,
} from "playwright";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { extractQrPayload } from "./dotli/helpers/extract-qr-payload";
import {
  formatSigningHostExit,
  startSigningHostPair,
  stopSigningHost,
  type SigningHostCliConfig,
  type SigningHostCliProcess,
} from "./dotli/helpers/signing-host-cli";

const currentDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(currentDir, "../../..");
const playgroundRoot = resolve(repoRoot, "playground");
const dotliRoot = resolve(
  process.env.E2E_DOTLI_ROOT ?? resolve(repoRoot, "hosts/dotli"),
);
const outputDir = resolve(playgroundRoot, "test-results/e2e-dotli");
const screenshotsDir = resolve(outputDir, "screenshots");

const hostPort = process.env.E2E_DOTLI_HOST_PORT ?? process.env.PORT ?? "5173";
const playgroundPort = process.env.E2E_DOTLI_PLAYGROUND_PORT ?? "3000";
const hostNetworks =
  process.env.E2E_DOTLI_NETWORKS ?? "paseo-next-v2,previewnet";
const headless = process.env.HEADED === "1" ? false : true;
const slowMo = process.env.SLOWMO ? Number(process.env.SLOWMO) : 0;
const smokeOnly = process.env.E2E_DOTLI_SMOKE === "1";
const loginUserBadgeTimeoutMs = Number(
  process.env.E2E_DOTLI_LOGIN_TIMEOUT_MS ?? "600000",
);
const signingHostNetwork =
  process.env.E2E_DOTLI_NETWORK ?? "paseo-next-v2";
const signingHostConfig: SigningHostCliConfig = {
  binary: resolve(
    process.env.E2E_DOTLI_SIGNING_HOST_BIN ??
      resolve(repoRoot, "target/debug/truapi-host"),
  ),
  cwd: repoRoot,
  basePath: resolve(
    process.env.E2E_DOTLI_SIGNING_HOST_BASE_PATH ??
      resolve(repoRoot, ".e2e-dotli/signing-host"),
  ),
  network: signingHostNetwork,
  liteUsernamePrefix: process.env.HOST_CLI_SIGNER_MNEMONIC?.trim()
    ? undefined
    : "dotlitest",
};
const expectedHostGaps = [
  "Account/create_account_proof",
  "Chat/create_room",
  "Chat/register_bot",
  "Chat/list_subscribe",
  "Chat/post_message",
  "Chat/action_subscribe",
  "Chat/custom_message_render_subscribe",
  "Coin Payment/create_purse",
  "Coin Payment/query_purse",
  "Coin Payment/rebalance_purse",
  "Coin Payment/delete_purse",
  "Coin Payment/create_receivable",
  "Coin Payment/create_cheque",
  "Coin Payment/deposit",
  "Coin Payment/refund",
  "Coin Payment/listen_for_payment",
  "Payment/balance_subscribe",
  "Payment/top_up",
  "Payment/request",
  "Payment/status_subscribe",
  // These generated examples ask for truapi-playground.dot while this E2E
  // loads the product as localhost:3000. Legacy payload/transaction methods
  // intentionally accept only the current product's slot-zero key.
  "Signing/create_transaction_with_legacy_account",
  "Signing/sign_payload_with_legacy_account",
];
const allowedFailures = new Set(
  (process.env.E2E_DOTLI_ALLOWED_FAILURES ?? expectedHostGaps.join(","))
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean),
);

const serverProcesses: ChildProcess[] = [];
const pageErrors: string[] = [];
const browserLogs: string[] = [];
const screenshots: string[] = [];
const signingHostLogs: string[] = [];
let screenshotSeq = 0;

interface SignedInSession {
  process: SigningHostCliProcess;
  username: string;
}

type PlaygroundE2E = {
  waitForConnectionStatus?: (
    status: "disconnected" | "connecting" | "connected",
    timeoutMs?: number,
  ) => Promise<string>;
  startAccountConnectionStatusProbe?: unknown;
};

declare global {
  interface Window {
    __dotliE2eAuthStates?: unknown[];
    __truapiPlaygroundE2E?: PlaygroundE2E;
    __TRUAPI_PLAYGROUND_E2E__?: boolean;
  }
}

function startServer(
  label: string,
  command: string,
  args: string[],
  cwd: string,
  env: NodeJS.ProcessEnv = {},
): ChildProcess {
  const child = spawn(command, args, {
    cwd,
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  serverProcesses.push(child);

  const prefix = `[${label}]`;
  child.stdout?.on("data", (chunk: Buffer) => {
    process.stdout.write(
      chunk
        .toString()
        .split("\n")
        .map((line) => (line.length > 0 ? `${prefix} ${line}` : line))
        .join("\n"),
    );
  });
  child.stderr?.on("data", (chunk: Buffer) => {
    process.stderr.write(
      chunk
        .toString()
        .split("\n")
        .map((line) => (line.length > 0 ? `${prefix} ${line}` : line))
        .join("\n"),
    );
  });
  child.on("exit", (code, signal) => {
    if (code !== null && code !== 0) {
      console.error(`${prefix} exited with code ${code}`);
    } else if (signal !== null && signal !== "SIGTERM") {
      console.error(`${prefix} exited via ${signal}`);
    }
  });
  return child;
}

async function waitForHttp(url: string, label: string): Promise<void> {
  const deadline = Date.now() + 120_000;
  let last = "";
  while (Date.now() < deadline) {
    for (const child of serverProcesses) {
      if (child.exitCode !== null) {
        throw new Error(`${label} failed to start; a server process exited`);
      }
    }
    try {
      const response = await fetch(url, { method: "GET" });
      if (response.ok) {
        return;
      }
      last = `${response.status} ${response.statusText}`;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
    }
    await sleep(1_000);
  }
  throw new Error(`${label} did not become ready at ${url}: ${last}`);
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function captureStep(page: Page, name: string): Promise<void> {
  const safeName = name.replace(/[^a-z0-9_-]+/gi, "-").toLowerCase();
  const filename = `${String(++screenshotSeq).padStart(2, "0")}-${safeName}.png`;
  const path = resolve(screenshotsDir, filename);
  mkdirSync(screenshotsDir, { recursive: true });
  await page
    .screenshot({ path, fullPage: true })
    .then(() => {
      screenshots.push(path);
      console.log(`[e2e-dotli] screenshot: ${path}`);
    })
    .catch((error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      console.warn(`[e2e-dotli] screenshot failed (${name}): ${message}`);
    });
}

async function assertPortsFree(): Promise<void> {
  await Promise.all([
    assertPortFree(Number(hostPort), "dotli preview"),
    assertPortFree(Number(playgroundPort), "playground"),
  ]);
}

async function assertPortFree(port: number, label: string): Promise<void> {
  try {
    await fetch(`http://127.0.0.1:${port}/`, { method: "GET" });
  } catch {
    return;
  }
  throw new Error(
    `${label} port ${port} is already serving HTTP. Stop the stale process or set ` +
      `${label === "dotli preview" ? "E2E_DOTLI_HOST_PORT" : "E2E_DOTLI_PLAYGROUND_PORT"}.`,
  );
}

async function startLocalStack(): Promise<void> {
  await assertPortsFree();
  startServer("dotli", "bun", ["run", "preview:debug"], dotliRoot, {
    PORT: hostPort,
    VITE_NETWORKS: hostNetworks,
  });
  startServer("playground", "yarn", ["dev"], playgroundRoot, {
    PORT: playgroundPort,
  });
  await Promise.all([
    waitForHttp(`http://localhost:${hostPort}/`, "dotli preview"),
    waitForHttp(`http://localhost:${playgroundPort}/`, "playground"),
  ]);
}

async function signOutIfNeeded(page: Page): Promise<void> {
  const badge = page.locator("#auth-button .user-badge");
  if (!(await badge.isVisible({ timeout: 2_000 }).catch(() => false))) {
    return;
  }
  console.log(
    "[e2e-dotli] existing session found; signing out through host UI",
  );
  await page.evaluate(() => {
    document.querySelector<HTMLButtonElement>("#auth-button")?.click();
  });
  await page.locator("#user-popover-disconnect").waitFor({
    state: "visible",
    timeout: 5_000,
  });
  await page.evaluate(() => {
    document
      .querySelector<HTMLButtonElement>("#user-popover-disconnect")
      ?.click();
  });
  await badge.waitFor({ state: "hidden", timeout: 20_000 });
}

async function openLoginQr(page: Page): Promise<string> {
  const auth = page.locator("#auth-button");
  await auth.waitFor({ state: "visible", timeout: 30_000 });
  await page.waitForFunction(
    () => !document.querySelector<HTMLButtonElement>("#auth-button")?.disabled,
    null,
    { timeout: 30_000 },
  );
  await auth.click();

  const qr = page.locator("#auth-modal-qr canvas");
  await qr.waitFor({ state: "visible", timeout: 30_000 });
  await captureStep(page, "login-qr");
  return await extractQrPayload(page, "#auth-modal-qr canvas");
}

async function signInWithSigningHost(page: Page): Promise<SignedInSession> {
  const handshake = await openLoginQr(page);
  const maxAttempts = 3;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    console.log(
      `[e2e-dotli] starting signing-host CLI pairing responder (${attempt}/${maxAttempts})`,
    );
    const signingHost = startSigningHostPair(signingHostConfig, handshake);
    try {
      const username = await waitForSignedIn(page, signingHost);
      await captureStep(page, "signed-in");
      return { process: signingHost, username };
    } catch (error) {
      signingHostLogs.push(signingHost.output());
      await stopSigningHost(signingHost);
      if (
        attempt === maxAttempts ||
        !isRetryableSigningHostPairError(error)
      ) {
        throw error;
      }
      console.warn(
        `[e2e-dotli] transient signing-host pairing failure; retrying in 5s (${attempt}/${maxAttempts})`,
      );
      await page.waitForTimeout(5_000);
    }
  }
  throw new Error("signing-host pairing retry exhausted");
}

function isRetryableSigningHostPairError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return (
    message.includes("Invalid Transaction") ||
    message.includes("temporarily banned") ||
    message.includes("timed out waiting for author_submitAndWatchExtrinsic")
  );
}

async function waitForSignedIn(
  page: Page,
  signingHost: SigningHostCliProcess,
): Promise<string> {
  try {
    const existingFailure = await latestLoginFailureReason(page);
    if (existingFailure !== null) {
      throw new Error(`Login failed: ${existingFailure}`);
    }
    const outcome = await Promise.race([
      page
        .locator("#auth-button .user-badge")
        .waitFor({ state: "visible", timeout: loginUserBadgeTimeoutMs })
        .then(() => ({ tag: "signed-in" as const })),
      page.evaluate(
        () =>
          new Promise<never>((_, reject) => {
            const listener = (event: Event): void => {
              const state = (
                event as CustomEvent<
                  { tag?: string; reason?: string } | undefined
                >
              ).detail;
              if (state?.tag !== "LoginFailed") {
                return;
              }
              window.removeEventListener("dotli:truapi-auth-state", listener);
              reject(new Error(`Login failed: ${state.reason ?? "unknown"}`));
            };
            window.addEventListener("dotli:truapi-auth-state", listener);
          }),
      ),
      signingHost.completed.then((result) => ({
        tag: "signing-host-exit" as const,
        result,
      })),
    ]);
    if (outcome.tag === "signing-host-exit") {
      throw new Error(formatSigningHostExit(outcome.result, signingHost.output()));
    }
    const username = (
      await page.locator("#user-popover-username").innerText()
    ).trim();
    if (username.length === 0) {
      throw new Error("signed-in host did not expose the account username");
    }
    return username;
  } catch (error) {
    await writeAuthDebug(page, {
      stage: "post-signing-host-pair-user-badge",
      signingHostOutput: signingHost.output(),
    });
    throw error;
  }
}

async function latestLoginFailureReason(page: Page): Promise<string | null> {
  return await page.evaluate(() => {
    const states = window.__dotliE2eAuthStates ?? [];
    for (let i = states.length - 1; i >= 0; i--) {
      const candidate = states[i] as {
        detail?: { tag?: string; reason?: string };
      };
      if (candidate.detail?.tag === "LoginFailed") {
        return candidate.detail.reason ?? "unknown";
      }
    }
    return null;
  });
}

async function writeAuthDebug(
  page: Page,
  extra: Record<string, unknown>,
): Promise<void> {
  const debug = await page.evaluate(() => {
    const safeStorageValue = (key: string, value: string): string => {
      if (
        key === "dotli:mode" ||
        key === "dotli:network" ||
        key === "dotli:chain-backend" ||
        key === "dotli:content-backend" ||
        key === "truapi:logLevel" ||
        key === "dotli:truapi-debug"
      ) {
        return value;
      }
      return "[redacted]";
    };
    const readStorage = (storage: Storage): Record<string, string> => {
      const values: Record<string, string> = {};
      for (let i = 0; i < storage.length; i++) {
        const key = storage.key(i);
        if (key !== null && /dotli|truapi/i.test(key)) {
          values[key] = safeStorageValue(key, storage.getItem(key) ?? "");
        }
      }
      return values;
    };

    const authButton = document.querySelector("#auth-button");
    const modal = document.querySelector("#auth-modal-backdrop");
    const modalReason = document.querySelector("#auth-modal-reason");
    return {
      url: location.href,
      authStates: window.__dotliE2eAuthStates ?? [],
      authButtonHtml: authButton?.outerHTML ?? null,
      authButtonText: authButton?.textContent ?? null,
      modalClass: modal?.getAttribute("class") ?? null,
      modalReasonText: modalReason?.textContent ?? null,
      localStorage: readStorage(localStorage),
      sessionStorage: readStorage(sessionStorage),
    };
  });
  const metadataPath = resolve(outputDir, "auth-debug.json");
  writeFileSync(
    metadataPath,
    `${JSON.stringify({ ...debug, ...extra }, null, 2)}\n`,
  );
  console.error(`[e2e-dotli] auth debug: ${metadataPath}`);
}

function startHostModalClicker(page: Page): () => void {
  let stopped = false;
  void (async () => {
    while (!stopped) {
      await acceptVisibleHostModal(page);
      await page.waitForTimeout(250).catch(() => {});
    }
  })();
  return () => {
    stopped = true;
  };
}

async function drainHostModals(page: Page, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (
      !(await page
        .locator(".signing-modal-backdrop")
        .isVisible()
        .catch(() => false))
    ) {
      return;
    }
    if (!(await acceptVisibleHostModal(page))) {
      await page.waitForTimeout(250);
    }
  }
  // A method can leave a modal stuck mid-flow (e.g. a remote signing that
  // never resolves). Cancel it so the report stays reachable; the method
  // already recorded its own failure.
  await dismissStuckHostModal(page);
}

async function dismissStuckHostModal(page: Page): Promise<void> {
  const buttons = page.locator(".signing-modal-backdrop button");
  const count = await buttons.count().catch(() => 0);
  for (let index = 0; index < count; index++) {
    const button = buttons.nth(index);
    const visible = await button.isVisible({ timeout: 100 }).catch(() => false);
    const enabled = await button.isEnabled({ timeout: 100 }).catch(() => false);
    if (!visible || !enabled) {
      continue;
    }
    const label = (await button.innerText().catch(() => "")).trim();
    if (label !== "Cancel" && label !== "Reject") {
      continue;
    }
    console.warn(`[e2e-dotli] dismissing stuck host modal via: ${label}`);
    await button.click({ timeout: 2_000 }).catch(() => {});
    return;
  }
}

async function acceptVisibleHostModal(page: Page): Promise<boolean> {
  const allowedLabels = new Set(["Allow", "Create", "Sign"]);
  const buttons = page.locator(".signing-modal-backdrop button");
  const count = await buttons.count().catch(() => 0);
  for (let index = 0; index < count; index++) {
    const button = buttons.nth(index);
    const visible = await button.isVisible({ timeout: 100 }).catch(() => false);
    const enabled = await button.isEnabled({ timeout: 100 }).catch(() => false);
    if (!visible || !enabled) {
      continue;
    }
    const label = (await button.innerText().catch(() => "")).trim();
    if (!allowedLabels.has(label)) {
      continue;
    }
    console.log(`[e2e-dotli] accepting host modal: ${label}`);
    await button.click({ timeout: 2_000 }).catch(() => {});
    return true;
  }
  return false;
}

async function findPlaygroundFrame(page: Page) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const frame = page
      .frames()
      .find((candidate) =>
        candidate.url().startsWith(`http://localhost:${playgroundPort}/`),
      );
    if (frame) {
      const ready = await frame
        .locator('[data-testid="diagnosis-entry"]')
        .isVisible({ timeout: 500 })
        .catch(() => false);
      if (ready) {
        return frame;
      }
    }
    await page.waitForTimeout(250);
  }
  throw new Error("playground iframe did not become ready");
}

async function waitForPlaygroundE2EHook(page: Page): Promise<void> {
  const frame = await findPlaygroundFrame(page);
  await frame.waitForFunction(() => Boolean(window.__truapiPlaygroundE2E), {
    timeout: 15_000,
  });
  await frame.evaluate(async () => {
    const hook = window.__truapiPlaygroundE2E;
    if (!hook?.waitForConnectionStatus) {
      throw new Error("playground e2e host connection hook is unavailable");
    }
    await hook.waitForConnectionStatus("connected", 30_000);
  });
}

async function assertHostSignOutAndReconnect(
  page: Page,
  previous: SignedInSession,
): Promise<SignedInSession> {
  console.log("[e2e-dotli] validating host sign-out");
  await signOutIfNeeded(page);
  await page
    .locator("#auth-button .user-badge")
    .waitFor({ state: "hidden", timeout: 20_000 });
  await stopSigningHost(previous.process);
  signingHostLogs.push(previous.process.output());
  await captureStep(page, "signed-out");

  console.log("[e2e-dotli] validating signer reconnect");
  const reconnected = await signInWithSigningHost(page);
  if (reconnected.username !== previous.username) {
    signingHostLogs.push(reconnected.process.output());
    await stopSigningHost(reconnected.process);
    throw new Error("signer reconnect returned a different account");
  }
  return reconnected;
}

async function runDiagnosis(page: Page): Promise<{
  summary: string;
  report: string;
  copyReportClicked: boolean;
  failedMethods: string[];
}> {
  for (let attempt = 1; attempt <= 2; attempt++) {
    try {
      return await runDiagnosisOnce(page);
    } catch (error) {
      if (attempt === 2 || !isFrameDetachedError(error)) {
        throw error;
      }
      console.warn(
        "[e2e-dotli] playground iframe detached during diagnosis; retrying once",
      );
      await captureStep(page, `diagnosis-frame-detached-attempt-${attempt}`);
      await page.waitForTimeout(1_000);
    }
  }
  throw new Error("diagnosis retry exhausted");
}

async function runDiagnosisOnce(page: Page): Promise<{
  summary: string;
  report: string;
  copyReportClicked: boolean;
  failedMethods: string[];
}> {
  const frame = await findPlaygroundFrame(page);
  await captureStep(page, "diagnosis-ready");
  await frame.locator('[data-testid="diagnosis-entry"]').click();
  await frame.locator('[data-testid="diagnosis-run"]').click();
  await captureStep(page, "diagnosis-running");

  await waitForDiagnosisReportReady(frame);

  const summary = await frame
    .locator('[data-testid="diagnosis-summary"]')
    .innerText({ timeout: 5_000 });
  await drainHostModals(page, 5_000);
  await captureStep(page, "diagnosis-report-ready");
  const report =
    (await frame
      .locator('[data-testid="diagnosis-report-markdown"]')
      .textContent({ timeout: 5_000 })) ?? "";
  if (report.trim().length === 0) {
    throw new Error("diagnosis report markdown is empty");
  }
  // Skipped methods render as failed (and appear failed in the matrix), but they
  // are intentional gaps — exclude them from the CI hard-fail gate so only
  // genuine failures fail the run.
  const failedMethods = await frame
    .locator(
      '[data-testid="diagnosis-row"][data-status="fail"]:not([data-skipped="true"]) .diag__name',
    )
    .allInnerTexts();

  await frame.locator('[data-testid="diagnosis-copy-report"]').click();

  return { summary, report, copyReportClicked: true, failedMethods };
}

async function waitForDiagnosisReportReady(frame: Frame): Promise<void> {
  const deadline = Date.now() + 20 * 60_000;
  let lastLogAt = 0;
  while (Date.now() < deadline) {
    const reportReady = await frame
      .locator('[data-testid="diagnosis-copy-report"]')
      .isVisible({ timeout: 1_000 });
    if (reportReady) {
      return;
    }

    const now = Date.now();
    if (now - lastLogAt >= 30_000) {
      lastLogAt = now;
      const progress = await frame.evaluate(() => {
        const counts: Record<string, number> = {};
        let running = "none";
        for (const row of document.querySelectorAll<HTMLElement>(
          '[data-testid="diagnosis-row"]',
        )) {
          const status = row.dataset.status ?? "unknown";
          counts[status] = (counts[status] ?? 0) + 1;
          if (status === "running") {
            running =
              row.querySelector<HTMLElement>(".diag__name")?.innerText ??
              "unknown";
          }
        }
        const parts = Object.entries(counts)
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([status, count]) => `${status}=${count}`);
        return `${parts.join(" ")} running=${running}`;
      });
      console.log(`[e2e-dotli] diagnosis progress: ${progress}`);
    }

    await sleep(5_000);
  }
  throw new Error("diagnosis did not finish within 20 minutes");
}

function isFrameDetachedError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("Frame was detached");
}

async function main(): Promise<void> {
  mkdirSync(outputDir, { recursive: true });
  mkdirSync(screenshotsDir, { recursive: true });
  console.log(`[e2e-dotli] host checkout=${dotliRoot}`);
  if (allowedFailures.size > 0) {
    console.log(
      `[e2e-dotli] allowed failures=${[...allowedFailures].join(", ")}`,
    );
  }
  if (!smokeOnly) {
    console.log(
      `[e2e-dotli] signing-host=${signingHostConfig.binary} network=${signingHostNetwork}`,
    );
    if (!existsSync(signingHostConfig.binary)) {
      throw new Error(
        `signing-host CLI not found at ${signingHostConfig.binary}; run cargo build -p truapi-host-cli`,
      );
    }
  } else {
    console.log("[e2e-dotli] smoke mode: validating local stack and QR only");
  }

  let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
  let context: BrowserContext | undefined;
  let signedInSession: SignedInSession | undefined;
  let page: Page | undefined;
  try {
    await startLocalStack();

    const executablePath = existsSync("/usr/bin/chromium")
      ? "/usr/bin/chromium"
      : undefined;
    browser = await chromium.launch({
      headless,
      slowMo,
      executablePath,
      args: ["--no-sandbox"],
    });
    context = await browser.newContext({
      serviceWorkers: "block",
      permissions: ["camera", "clipboard-read", "clipboard-write"],
    });
    page = await context.newPage();
    page.on("pageerror", (error) => {
      const message = error.stack ?? error.message;
      pageErrors.push(message);
      console.error(`[browser:pageerror] ${message}`);
    });
    page.on("console", (message) => {
      const text = message.text();
      if (
        message.type() === "error" ||
        /\[truapi|\[dotli|\[dot\.li|statement.store|signing/i.test(text)
      ) {
        const line = `[browser:${message.type()}] ${text}`;
        browserLogs.push(line);
        console.log(line);
      }
    });

    await page.addInitScript(
      ({ playgroundPort }) => {
        try {
          const playgroundLabel = `localhost:${playgroundPort}`;
          localStorage.setItem("dotli:mode", "gateway");
          localStorage.setItem("dotli:chain-backend", "rpc-gateway");
          localStorage.setItem("dotli:content-backend", "ipfs-gateway");
          localStorage.setItem(
            `dotli:permissions:${playgroundLabel}`,
            JSON.stringify({ Camera: "granted" }),
          );
          localStorage.setItem("desktop-banner-dismissed", "1");
          sessionStorage.removeItem("dotli:truapi-debug");
          localStorage.setItem("truapi:logLevel", "debug");
          localStorage.setItem("truapi:playground:e2e", "1");
          window.__TRUAPI_PLAYGROUND_E2E__ = true;
          window.__dotliE2eAuthStates = [];
          window.addEventListener("dotli:truapi-auth-state", (event: Event) => {
            window.__dotliE2eAuthStates?.push({
              timestamp: Date.now(),
              detail: (event as CustomEvent<unknown>).detail,
            });
          });
        } catch {
          /* ignore */
        }
      },
      { playgroundPort },
    );

    const params = new URLSearchParams({
      chainBackend: "rpc-gateway",
      e2e: String(Date.now()),
      network: signingHostNetwork,
    });
    const url = `http://localhost:${hostPort}/localhost:${playgroundPort}?${params.toString()}`;
    await page.goto(url, { timeout: 60_000, waitUntil: "domcontentloaded" });
    await captureStep(page, "loaded");
    await signOutIfNeeded(page);
    if (smokeOnly) {
      const handshake = await openLoginQr(page);
      const metadataPath = resolve(outputDir, "smoke-run.json");
      writeFileSync(
        metadataPath,
        `${JSON.stringify(
          {
            mode: "smoke",
            hostCheckout: dotliRoot,
            handshakePrefix: handshake.slice(0, 32),
            pageErrors,
            browserLogs,
            timestamp: new Date().toISOString(),
          },
          null,
          2,
        )}\n`,
      );
      console.log(`[e2e-dotli] smoke complete: ${metadataPath}`);
      if (pageErrors.length > 0) {
        throw new Error(`browser page errors occurred: ${pageErrors.length}`);
      }
      return;
    }
    await waitForPlaygroundE2EHook(page);
    signedInSession = await signInWithSigningHost(page);
    signedInSession = await assertHostSignOutAndReconnect(
      page,
      signedInSession,
    );
    const stopClicker = startHostModalClicker(page);
    try {
      const { summary, report, copyReportClicked, failedMethods } =
        await runDiagnosis(page);
      const reportPath = resolve(outputDir, "diagnosis-report.md");
      writeFileSync(reportPath, report);
      const allowedFailedMethods = failedMethods.filter((method) =>
        allowedFailures.has(method),
      );
      const unexpectedFailedMethods = failedMethods.filter(
        (method) => !allowedFailures.has(method),
      );
      const metadataPath = resolve(outputDir, "diagnosis-run.json");
      writeFileSync(
        metadataPath,
        `${JSON.stringify(
          {
            summary,
            failedMethods,
            allowedFailedMethods,
            unexpectedFailedMethods,
            hostCheckout: dotliRoot,
            reportPath,
            copyReportClicked,
            screenshots,
            user: {
              username: signedInSession.username,
              network: signingHostNetwork,
            },
            signingHost: {
              binary: signingHostConfig.binary,
              basePath: signingHostConfig.basePath,
              output: [
                ...signingHostLogs,
                signedInSession.process.output(),
              ],
            },
            sessionLifecycle: "host-sign-out-reconnect",
            pageErrors,
            browserLogs,
            timestamp: new Date().toISOString(),
          },
          null,
          2,
        )}\n`,
      );
      console.log(`[e2e-dotli] diagnosis complete: ${summary}`);
      console.log(`[e2e-dotli] report: ${reportPath}`);
      if (allowedFailedMethods.length > 0) {
        console.warn(
          `[e2e-dotli] allowed failures observed: ${allowedFailedMethods.join(", ")}`,
        );
      }
      if (unexpectedFailedMethods.length > 0) {
        throw new Error(
          `diagnosis reported unexpected failed methods: ${unexpectedFailedMethods.join(", ")}`,
        );
      }
      if (pageErrors.length > 0) {
        throw new Error(`browser page errors occurred: ${pageErrors.length}`);
      }
    } finally {
      stopClicker();
    }
  } catch (error) {
    if (page) {
      await captureStep(page, "failure");
    }
    throw error;
  } finally {
    if (signedInSession) {
      signingHostLogs.push(signedInSession.process.output());
      await stopSigningHost(signedInSession.process);
    }
    await context?.close().catch(() => {});
    await browser?.close().catch(() => {});
    for (const child of serverProcesses) {
      child.kill("SIGTERM");
    }
  }
}

main().catch((error: unknown) => {
  const message =
    error instanceof Error ? (error.stack ?? error.message) : String(error);
  mkdirSync(dirname(resolve(outputDir, "failure.log")), { recursive: true });
  writeFileSync(resolve(outputDir, "failure.log"), `${message}\n`);
  console.error(`[e2e-dotli] ${message}`);
  process.exit(1);
});
