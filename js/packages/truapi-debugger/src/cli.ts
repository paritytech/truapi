// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * `truapi-debugger` terminal frontend: look at wire traces from a shell, for
 * headless / SSH / CI workflows where the web inspector isn't reachable.
 *
 * Two frontends over one running debugger (`:9231` by default), sharing the same
 * {@link module:cli-client} engine and the same sensitive denylist as the web
 * inspector - no forked engine, no forked denylist:
 *
 *  - `ui` / `repl` (default in a terminal): the interactive query {@link module:repl}
 *    - a prompt you keep querying: ls, filter, sort, use <channel>, show, reveal.
 *  - `ls` / `stats` / `show` / `tail`: one-shot commands for scripting + piping.
 *
 * Usage (from js/packages/truapi-debugger):
 *   bun run src/cli.ts                    # interactive query REPL
 *   bun run src/cli.ts ls                 # ops + aggregate summary
 *   bun run src/cli.ts stats              # just the aggregate line
 *   bun run src/cli.ts show p:4 --reveal  # one op's frames + decoded values
 *   bun run src/cli.ts tail               # live view, refreshes each second
 * Flags: --host http://localhost:9231 · --channel <id> · --reveal · --interval <ms>
 *
 * @module
 */

import {
  createDebuggerClient,
  toView,
  type FrameValueDetail,
  type TracesEntry,
} from "./cli-client.js";
import { formatOpDetail, formatOpRow, formatStats } from "./trace-text.js";
import { runRepl } from "./repl.js";

interface ParsedArgs {
  cmd: string;
  positional: string[];
  flags: Record<string, string | boolean>;
}

/** Flags that take a following value; everything else is a boolean flag. */
const VALUE_FLAGS = new Set(["host", "channel", "interval"]);

function parseArgs(argv: string[]): ParsedArgs {
  const flags: Record<string, string | boolean> = {};
  const positional: string[] = [];
  let cmd = "";
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      // Only value-flags consume the next token; a boolean flag (e.g. --reveal)
      // leaves it as a positional, so `show --reveal p:4` parses correctly.
      if (VALUE_FLAGS.has(key) && next !== undefined && !next.startsWith("--")) {
        flags[key] = next;
        i++;
      } else {
        flags[key] = true;
      }
    } else if (cmd === "") {
      cmd = a;
    } else {
      positional.push(a);
    }
  }
  // No command in an interactive terminal → the query REPL; otherwise the list.
  if (cmd === "") cmd = process.stdout.isTTY ? "ui" : "ls";
  return { cmd, positional, flags };
}

const args = parseArgs(process.argv.slice(2));
// A bare `--host`/`--channel` (no value) parses as boolean `true`; take only a
// real string value as provided, otherwise fall back rather than coerce garbage.
const flagValue = (v: string | boolean | undefined): string | undefined =>
  typeof v === "string" ? v : undefined;
const host =
  flagValue(args.flags.host) ??
  process.env.TRUAPI_DEBUGGER_HTTP ??
  "http://localhost:9231";
const channel = flagValue(args.flags.channel) ?? null;
const reveal = args.flags.reveal === true || args.flags.reveal === "1";
const client = createDebuggerClient(host);

async function traces(): Promise<TracesEntry[]> {
  const all = await client.traces();
  return channel === null ? all : all.filter((t) => t.channelId === channel);
}

async function cmdStats(): Promise<void> {
  console.log(formatStats(await client.stats(channel)));
}

async function cmdLs(): Promise<void> {
  const [stats, entries] = await Promise.all([client.stats(channel), traces()]);
  console.log(formatStats(stats));
  console.log("");
  if (entries.length === 0) console.log("  (no operations yet)");
  // Unscoped view: show the channel so same-id ops from two hosts are distinct.
  for (const t of entries) console.log(formatOpRow(toView(t), channel === null));
}

async function cmdShow(): Promise<void> {
  const id = args.positional[0];
  if (id === undefined) {
    console.error("usage: show <requestId> [--reveal] [--channel <id>]");
    process.exit(1);
  }
  const entry = (await traces()).find((t) => t.requestId === id);
  if (entry === undefined) {
    console.error(`no operation with requestId ${id}`);
    process.exit(1);
  }
  const view = toView(entry);
  if (reveal) {
    // The one-shot reveal is a deliberate, non-interactive scripting path (the
    // interactive REPL uses a typed `reveal <id>` + `yes` confirm instead). Warn
    // up front as the REPL does; the server still only honors reveal when armed.
    console.error(
      "\x1b[31m⚠ revealing SENSITIVE payloads\x1b[0m\x1b[2m — output may contain a private key, signature, or credential; do NOT run this while screen-sharing or recording. Honored only on a server armed with TRUAPI_DEBUGGER_REVEAL_SENSITIVE.\x1b[0m",
    );
  }
  const decoded = new Map<number, FrameValueDetail>();
  for (const f of view.frames) {
    try {
      decoded.set(
        f.seq,
        await client.frame(entry.requestId, f.seq, entry.channelId, reveal),
      );
    } catch {
      // Leave the frame value-less; the row still renders.
    }
  }
  console.log(formatOpDetail(view, decoded));
}

async function cmdTail(): Promise<void> {
  const interval = Number(args.flags.interval ?? 1000);
  const render = async (): Promise<void> => {
    const [stats, entries] = await Promise.all([client.stats(channel), traces()]);
    process.stdout.write("\x1b[2J\x1b[H");
    console.log(formatStats(stats));
    console.log("");
    for (const t of entries.slice(-40)) console.log(formatOpRow(toView(t)));
    console.log(
      `\n\x1b[2mwatching ${host}${channel ? ` · ${channel}` : ""} — Ctrl-C to stop\x1b[0m`,
    );
  };
  await render();
  setInterval(() => {
    render().catch((e: unknown) => {
      console.error(e instanceof Error ? e.message : String(e));
    });
  }, interval);
}

async function cmdUi(): Promise<void> {
  await runRepl(client, channel);
}

const commands: Record<string, () => Promise<void>> = {
  ui: cmdUi,
  repl: cmdUi,
  stats: cmdStats,
  ls: cmdLs,
  ops: cmdLs,
  show: cmdShow,
  tail: cmdTail,
  watch: cmdTail,
};

const run = commands[args.cmd];
if (run === undefined) {
  console.error(
    `unknown command: ${args.cmd}\ncommands: ui · stats · ls · show <requestId> · tail`,
  );
  process.exit(1);
}
run().catch((e: unknown) => {
  console.error(e instanceof Error ? e.message : String(e));
  process.exit(1);
});
