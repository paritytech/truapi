// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { setTimeout as sleep } from "node:timers/promises";

const TRANSIENT = new Set([502, 503, 504]);

// Per-attempt request timeout. First-time pair can include user creation,
// People-chain attestation, and V2 pairing-time device allowance finalization.
// Give the one-shot handshake room to finish instead of aborting and retrying
// the same QR payload.
const PAIR_REQUEST_TIMEOUT_MS = Number(
  process.env.SIGNER_BOT_PAIR_TIMEOUT_MS ?? "240000",
);
const HEALTH_REQUEST_TIMEOUT_MS = 5_000;

function shellQuote(value: string): string {
  if (value.length === 0) return "''";
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function buildPairCurl(url: string, body: unknown): string {
  return [
    `curl -sS -X POST ${shellQuote(url)}`,
    '-H "Authorization: Bearer ${SIGNER_BOT_SVC_TOKEN}"',
    "-H 'Content-Type: application/json'",
    "-H 'Accept: application/json'",
    `--data-raw ${shellQuote(JSON.stringify(body))}`,
  ].join(" \\\n  ");
}

function redactSignerBotResponse(text: string): string {
  return text.replace(/"mnemonic"\s*:\s*"[^"]+"/g, '"mnemonic":"[redacted]"');
}

async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs: number,
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

interface TextResponse {
  response: Response;
  text: string;
}

async function fetchTextWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs: number,
): Promise<TextResponse> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, { ...init, signal: controller.signal });
    const text = await response.text();
    return { response, text };
  } finally {
    clearTimeout(timer);
  }
}

async function fetchTextRetry(
  url: string,
  init: RequestInit,
  attempts = 4,
  timeoutMs = PAIR_REQUEST_TIMEOUT_MS,
): Promise<TextResponse> {
  let last: unknown = null;
  for (let i = 1; i <= attempts; i++) {
    try {
      const result = await fetchTextWithTimeout(url, init, timeoutMs);
      const { response, text } = result;
      if (response.ok || !TRANSIENT.has(response.status) || i === attempts) {
        return result;
      }
      console.warn(
        `[bot] ${init.method ?? "GET"} ${url} response ${response.status} ${response.statusText} (attempt ${i}/${attempts}): ${redactSignerBotResponse(text)}`,
      );
    } catch (e) {
      last = e;
      if ((e as Error).name === "AbortError") throw e;
      if (i === attempts) throw e;
      console.warn(
        `[bot] ${init.method ?? "GET"} ${url} threw "${(e as Error).message}" (attempt ${i}/${attempts})`,
      );
    }
    await sleep(1_000 * 2 ** (i - 1));
  }
  throw last ?? new Error("fetchTextRetry exhausted");
}

/**
 * Generate a per-run username for the Nova signing bot.
 *
 * Each test run gets its own throwaway user so network state (allowances,
 * permissions, derived product accounts) doesn't leak between PRs. The
 * format is `dotlitests` followed by 6 lowercase letters, a namespace of
 * roughly 3·10^8 with no collisions in practice. It stays inside the bot's
 * strictest username regex (^[a-z]+$) so it works for both regular
 * `username` and `liteUsername` fields if we ever want one.
 */
export function generateUsername(): string {
  const alphabet = "abcdefghijklmnopqrstuvwxyz";
  let suffix = "";
  for (let i = 0; i < 6; i++) {
    suffix += alphabet[Math.floor(Math.random() * alphabet.length)];
  }
  return `dotlitests${suffix}`;
}

export interface PairResult {
  sessionId: string;
  user: {
    username: string;
    network: string;
    address: string;
    publicKeyHex: string;
    attested?: boolean;
  };
}

/**
 * Pair the bot with a dot.li session via the QR-derived handshake deeplink.
 *
 * The Nova bot's `/api/pair` is one-shot: given a handshake, it
 * (a) creates the user if `username` is new, (b) attests the account on
 * People chain so it has Statement Store allowance, (c) submits V2
 * pairing-time device allowance, (d) completes the SSO handshake, and
 * (e) starts auto-signing future SignRequests for that session.
 *
 * `network` should match dot.li's default network (`paseo-next-v2` at time
 * of writing). The bot's `/api/networks` endpoint lists supported IDs.
 */
export async function pair(
  base: string,
  svcToken: string,
  args: { handshake: string; username: string; network: string },
): Promise<PairResult> {
  const url = `${base.replace(/\/$/, "")}/api/pair`;
  console.log(`[bot] /api/pair curl:\n${buildPairCurl(url, args)}`);
  const init: RequestInit = {
    method: "POST",
    headers: {
      Authorization: `Bearer ${svcToken}`,
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify(args),
  };
  let result: TextResponse;
  try {
    result = await fetchTextRetry(url, init);
  } catch (e) {
    console.error(
      `[bot] /api/pair response unavailable: ${(e as Error).message}`,
    );
    throw e;
  }
  const { response: r, text } = result;
  console.log(
    `[bot] /api/pair raw response ${r.status} ${r.statusText}: ${redactSignerBotResponse(text) || "<empty>"}`,
  );
  if (!r.ok) {
    throw new Error(`pair ${r.status}: ${text}`);
  }
  return JSON.parse(text) as PairResult;
}

/**
 * Tear down a bot session at the end of a test worker.
 *
 * Best-effort. Failure to disconnect is non-fatal, the bot times
 * sessions out anyway.
 */
export async function disconnect(
  base: string,
  svcToken: string,
  sessionId: string,
): Promise<void> {
  await disconnectStrict(base, svcToken, sessionId).catch(() => {});
}

export async function disconnectStrict(
  base: string,
  svcToken: string,
  sessionId: string,
): Promise<void> {
  const response = await fetchWithTimeout(
    `${base.replace(/\/$/, "")}/api/disconnect`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${svcToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ sessionId }),
    },
    PAIR_REQUEST_TIMEOUT_MS,
  );
  if (!response.ok) {
    throw new Error(`disconnect ${response.status}: ${await response.text()}`);
  }
  const body = (await response.json()) as { disconnected?: boolean };
  if (body.disconnected !== true) {
    throw new Error(`disconnect did not find session ${sessionId}`);
  }
}

export interface BotHealth {
  ok: boolean;
  status?: string;
  uptime?: number;
  error?: string;
}

/**
 * Lightweight bot reachability probe. Used by globalSetup to fail-fast
 * when the bot is unreachable, distinguishing "Nova is down" from
 * "dot.li is broken" in CI output. No auth required.
 */
export async function health(base: string): Promise<BotHealth> {
  try {
    const r = await fetchWithTimeout(
      `${base.replace(/\/$/, "")}/api/health`,
      { method: "GET", headers: { Accept: "application/json" } },
      HEALTH_REQUEST_TIMEOUT_MS,
    );
    if (!r.ok) {
      return { ok: false, error: `${r.status} ${r.statusText}` };
    }
    const body = (await r.json()) as { status?: string; uptime?: number };
    return {
      ok: body.status === "ok",
      status: body.status,
      uptime: body.uptime,
    };
  } catch (e) {
    return { ok: false, error: (e as Error).message };
  }
}
