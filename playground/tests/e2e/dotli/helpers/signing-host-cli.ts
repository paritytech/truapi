// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { spawn, type ChildProcess } from "node:child_process";
import type { Readable, Writable } from "node:stream";

const MAX_CAPTURED_OUTPUT_BYTES = 256 * 1024;
const PAIRING_DEEPLINK = /polkadotapp:\/\/pair\?[^\s'"]+/g;

export interface SigningHostCliConfig {
  binary: string;
  cwd: string;
  basePath: string;
  network: string;
  liteUsernamePrefix?: string;
}

export interface SigningHostExit {
  code: number | null;
  signal: NodeJS.Signals | null;
  error?: string;
}

export interface SigningHostCliProcess {
  child: ChildProcess;
  completed: Promise<SigningHostExit>;
  output: () => string;
}

export function signingHostPairArgs(
  config: SigningHostCliConfig,
  deeplink: string,
): string[] {
  const args = [
    "signing-host",
    "--network",
    config.network,
    "--base-path",
    config.basePath,
    "--frame-listen",
    "127.0.0.1:0",
    "--auto-accept",
  ];
  if (config.liteUsernamePrefix !== undefined) {
    args.push("--lite-username-prefix", config.liteUsernamePrefix);
  }
  args.push("exec", `/pair ${deeplink}`);
  return args;
}

export function sanitizeSigningHostOutput(text: string): string {
  return text.replace(PAIRING_DEEPLINK, "<pairing deeplink>");
}

export function startSigningHostPair(
  config: SigningHostCliConfig,
  deeplink: string,
): SigningHostCliProcess {
  const child = spawn(
    config.binary,
    signingHostPairArgs(config, deeplink),
    {
      cwd: config.cwd,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let captured = "";
  const append = (line: string): void => {
    captured = `${captured}${line}\n`;
    if (Buffer.byteLength(captured) > MAX_CAPTURED_OUTPUT_BYTES) {
      captured = captured.slice(-MAX_CAPTURED_OUTPUT_BYTES);
    }
  };
  pipeLines(child.stdout, process.stdout, "[signing-host]", append);
  pipeLines(child.stderr, process.stderr, "[signing-host]", append);

  const completed = new Promise<SigningHostExit>((resolve) => {
    child.once("error", (error) => {
      resolve({ code: null, signal: null, error: error.message });
    });
    child.once("exit", (code, signal) => {
      resolve({ code, signal });
    });
  });

  return {
    child,
    completed,
    output: () => captured.trimEnd(),
  };
}

export async function stopSigningHost(
  process: SigningHostCliProcess,
): Promise<void> {
  if (process.child.exitCode !== null || process.child.signalCode !== null) {
    return;
  }
  process.child.kill("SIGTERM");
  const stopped = await Promise.race([
    process.completed.then(() => true),
    new Promise<boolean>((resolve) => {
      const timer = setTimeout(() => resolve(false), 5_000);
      timer.unref();
    }),
  ]);
  if (!stopped && process.child.exitCode === null) {
    process.child.kill("SIGKILL");
    await process.completed;
  }
}

export function formatSigningHostExit(
  result: SigningHostExit,
  output: string,
): string {
  const status =
    result.error ??
    (result.code !== null
      ? `exit code ${result.code}`
      : `signal ${result.signal ?? "unknown"}`);
  const detail = sanitizeSigningHostOutput(output).trim();
  return detail.length > 0
    ? `signing-host stopped before login (${status}):\n${detail}`
    : `signing-host stopped before login (${status})`;
}

function pipeLines(
  stream: Readable | null,
  destination: Writable,
  prefix: string,
  append: (line: string) => void,
): void {
  if (stream === null) {
    return;
  }
  stream.setEncoding("utf8");
  let buffered = "";
  const emit = (line: string): void => {
    const sanitized = sanitizeSigningHostOutput(line);
    append(sanitized);
    destination.write(`${prefix} ${sanitized}\n`);
  };
  stream.on("data", (chunk: string) => {
    buffered += chunk;
    const lines = buffered.split(/\r?\n/);
    buffered = lines.pop() ?? "";
    for (const line of lines) {
      if (line.length > 0) {
        emit(line);
      }
    }
  });
  stream.on("end", () => {
    if (buffered.length > 0) {
      emit(buffered);
    }
  });
}
