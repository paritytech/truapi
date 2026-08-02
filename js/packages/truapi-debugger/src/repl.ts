// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Interactive query REPL for the wire debugger - a prompt you keep talking to,
 * rather than a full-screen app. Line-based (via `node:readline`, so history and
 * line editing come for free), over a running debugger, reusing the same
 * {@link buildTraceView} engine and denylist as the web inspector.
 *
 * Session scope (channel / filter / sort / sensitive-only) persists across
 * queries, so `ls` reflects the state you set. The sensitive-reveal escape hatch
 * is a two-step, in-loop confirm (`reveal <id>` then `yes`) - no nested prompt,
 * and the reveal is honored only when the server is armed.
 *
 * @module
 */

import readline from "node:readline";

import {
  toView,
  viewMethod,
  type DebuggerClient,
  type FrameValueDetail,
} from "./cli-client.js";
import type { TraceView } from "./trace-view.js";
import { formatOpDetail, formatOpRow, formatStats } from "./trace-text.js";

const COLOR =
  process.env.NO_COLOR === undefined && process.stdout.isTTY === true;
function c(code: string, s: string): string {
  return COLOR ? `\x1b[${code}m${s}\x1b[0m` : s;
}
const bold = (s: string): string => c("1", s);
const dim = (s: string): string => c("2", s);
const red = (s: string): string => c("31", s);
const green = (s: string): string => c("32", s);
const cyan = (s: string): string => c("36", s);

const SORTS = ["arrival", "recent", "method", "duration", "frames"];

interface ReplState {
  channel: string | null;
  filter: string;
  sort: string;
  sensOnly: boolean;
  /** A reveal awaiting the next line's `yes` confirmation. */
  pendingReveal: { requestId: string; seq?: number } | null;
}

const HELP = [
  bold("commands"),
  `  ${cyan("ls")} [text]            list ops (aggregate + rows); optional inline method filter`,
  `  ${cyan("stats")}               just the aggregate summary line`,
  `  ${cyan("show")} <id>           an op's frames, decoding non-sensitive values`,
  `  ${cyan("decode")} <id>         alias for show`,
  `  ${cyan("reveal")} <id> [seq]   reveal sensitive frame(s) — asks to confirm (dev, armed server only)`,
  `  ${cyan("channels")}            hosts that have dialed in`,
  `  ${cyan("use")} <channel|all>   scope every query to one channel`,
  `  ${cyan("filter")} [text]       persistent method filter (empty clears)`,
  `  ${cyan("sort")} <mode>         ${SORTS.join(" | ")}`,
  `  ${cyan("sensitive")} [on|off]  show only ops with a sensitive method`,
  `  ${cyan("clear")}               clear the screen`,
  `  ${cyan("help")} · ${cyan("quit")}`,
].join("\n");

/** Run the query REPL against `client`. Resolves when the user quits. */
export async function runRepl(
  client: DebuggerClient,
  channel: string | null,
): Promise<void> {
  const state: ReplState = {
    channel,
    filter: "",
    sort: "arrival",
    sensOnly: false,
    pendingReveal: null,
  };
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    historySize: 200,
    terminal: process.stdin.isTTY === true,
  });

  console.log(`${bold("TrUAPI wire debugger")}${dim(` — ${client.host}`)}`);
  console.log(dim("type `help` for commands, `quit` to exit"));

  const promptStr = (): string => {
    const bits = [state.channel ?? "all"];
    if (state.filter) bits.push(cyan(`/${state.filter}`));
    if (state.sort !== "arrival") bits.push(`sort:${state.sort}`);
    if (state.sensOnly) bits.push(red("\u{1f512}"));
    return `${green("truapi")} ${dim(bits.join(" "))} ${bold("▸")} `;
  };

  function sortViews(views: TraceView[]): TraceView[] {
    if (state.sort === "arrival") return views;
    return [...views].sort((a, b) => {
      switch (state.sort) {
        case "recent":
          return b.lastAt - a.lastAt;
        case "duration":
          return b.durationMs - a.durationMs;
        case "frames":
          return b.frames.length - a.frames.length;
        case "method":
          return viewMethod(a).localeCompare(viewMethod(b));
        default:
          return 0;
      }
    });
  }

  async function views(inlineFilter?: string): Promise<TraceView[]> {
    const all = await client.traces();
    let vs = all
      .filter((t) => state.channel === null || t.channelId === state.channel)
      .map(toView);
    const f = (inlineFilter ?? state.filter).toLowerCase();
    if (f) vs = vs.filter((v) => viewMethod(v).toLowerCase().includes(f));
    if (state.sensOnly) vs = vs.filter((v) => v.sensitive === true);
    return sortViews(vs);
  }

  async function doList(inlineFilter?: string): Promise<void> {
    const [stats, vs] = await Promise.all([
      client.stats(state.channel),
      views(inlineFilter),
    ]);
    console.log(formatStats(stats));
    console.log("");
    if (vs.length === 0) console.log(dim("  (no operations match)"));
    // Unscoped view: show the channel so same-id ops from two hosts are distinct.
    for (const v of vs) console.log(formatOpRow(v, state.channel === null));
  }

  async function doChannels(): Promise<void> {
    const chs = await client.channels();
    if (chs.length === 0) {
      console.log(dim("  (no hosts have dialed in yet)"));
      return;
    }
    for (const ch of chs) {
      console.log(
        `${ch.connected ? green("●") : dim("○")} ${ch.channelId} ${dim(`(${String(ch.frameCount)} frames)`)}${ch.channelId === state.channel ? cyan("  ← scoped") : ""}`,
      );
    }
  }

  async function findOp(id: string) {
    return (await client.traces()).find(
      (t) =>
        t.requestId === id &&
        (state.channel === null || t.channelId === state.channel),
    );
  }

  async function doShow(id: string, revealSeqs?: Set<number>): Promise<void> {
    const entry = await findOp(id);
    if (entry === undefined) {
      console.log(red(`no operation with requestId ${id}`));
      return;
    }
    const view = toView(entry);
    const decoded = new Map<number, FrameValueDetail>();
    for (const f of view.frames) {
      const reveal = revealSeqs?.has(f.seq) ?? false;
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

  async function startReveal(id: string, seqArg?: string): Promise<void> {
    const entry = await findOp(id);
    if (entry === undefined) {
      console.log(red(`no operation with requestId ${id}`));
      return;
    }
    const view = toView(entry);
    const seq = seqArg === undefined ? undefined : Number(seqArg);
    const targets =
      seq === undefined
        ? view.frames.filter((f) => f.sensitive === true)
        : view.frames.filter((f) => f.seq === seq);
    if (targets.length === 0) {
      console.log(dim("  (no sensitive frame to reveal here)"));
      return;
    }
    state.pendingReveal = { requestId: id, seq };
    console.log(
      red("⚠ reveal SENSITIVE payload") +
        dim(" — may contain a private key/credential; not while screen-sharing.\n") +
        `  type ${bold("yes")} to confirm (anything else cancels)`,
    );
  }

  async function handle(line: string): Promise<void> {
    // A pending reveal consumes this line as its confirmation.
    if (state.pendingReveal) {
      const { requestId, seq } = state.pendingReveal;
      state.pendingReveal = null;
      if (line.toLowerCase() !== "yes" && line.toLowerCase() !== "y") {
        console.log(dim("  (reveal cancelled)"));
        return;
      }
      const entry = await findOp(requestId);
      if (entry === undefined) {
        console.log(red(`no operation with requestId ${requestId}`));
        return;
      }
      const view = toView(entry);
      // A specific seq reveals just that frame; otherwise every sensitive frame.
      const revealSeqs =
        seq === undefined
          ? new Set(view.frames.filter((f) => f.sensitive === true).map((f) => f.seq))
          : new Set([seq]);
      await doShow(requestId, revealSeqs);
      return;
    }

    const [cmd, ...rest] = line.split(/\s+/).filter(Boolean);
    if (cmd === undefined) return;
    const pos = rest.filter((a) => !a.startsWith("--"));
    const arg = pos[0];
    switch (cmd) {
      case "help":
      case "?":
        console.log(HELP);
        return;
      case "ls":
      case "ops":
        return doList(arg);
      case "stats":
        console.log(formatStats(await client.stats(state.channel)));
        return;
      case "channels":
        return doChannels();
      case "show":
      case "decode":
        if (arg === undefined) {
          console.log(dim("usage: show <requestId>"));
          return;
        }
        return doShow(arg);
      case "reveal":
        if (arg === undefined) {
          console.log(dim("usage: reveal <requestId> [seq]"));
          return;
        }
        return startReveal(arg, pos[1]);
      case "use":
      case "channel":
        state.channel = arg === undefined || arg === "all" ? null : arg;
        return;
      case "filter":
        state.filter = rest.filter((a) => !a.startsWith("--")).join(" ");
        return;
      case "sort":
        if (arg !== undefined && SORTS.includes(arg)) state.sort = arg;
        else console.log(dim(`sort: ${SORTS.join(" | ")}`));
        return;
      case "sensitive":
      case "sens":
        state.sensOnly = arg === undefined ? !state.sensOnly : arg === "on";
        return;
      case "clear":
        console.clear();
        return;
      case "quit":
      case "exit":
      case "q":
        rl.close();
        return;
      default:
        console.log(dim(`unknown command: ${cmd} — try \`help\``));
    }
  }

  const prompt = (): void => {
    rl.setPrompt(promptStr());
    rl.prompt();
  };

  // Serialize line handling so piped input and in-flight fetches never interleave.
  let chain: Promise<void> = Promise.resolve();
  prompt();
  rl.on("line", (line) => {
    chain = chain
      .then(() => handle(line.trim()))
      .catch((e: unknown) => {
        console.error(red(e instanceof Error ? e.message : String(e)));
      })
      .then(() => prompt());
  });

  await new Promise<void>((resolve) => {
    rl.on("close", () => {
      console.log(dim("bye"));
      resolve();
    });
  });
}
