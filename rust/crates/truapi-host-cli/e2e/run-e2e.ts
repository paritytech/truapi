// End-to-end orchestrator: relay + pairing host + signing host + product driver.
//
// Boots the dev statement-store relay, a headless pairing host (frame server),
// connects the real @parity/truapi client, starts login, hands the pairing
// deeplink to a headless signing host, and once paired runs the signing
// battery. Prints a pass/fail summary and exits non-zero on any failure.
import { resolve } from "node:path";
import { readFileSync } from "node:fs";
import { beginLogin, connect, runBattery, type CaseResult } from "./driver.ts";
import { runDiagnosis, type DiagnosisRow } from "./diagnosis.ts";

// Load `e2e/.env` (gitignored) so a registered signer mnemonic can be kept
// out of the command line. Existing environment variables win.
function loadDotenv() {
  try {
    for (const line of readFileSync(resolve(import.meta.dir, ".env"), "utf8").split("\n")) {
      const match = line.match(/^\s*([A-Z0-9_]+)\s*=\s*(.*)\s*$/);
      if (!match || line.trimStart().startsWith("#")) continue;
      const key = match[1];
      const value = match[2].replace(/^["']|["']$/g, "");
      if (process.env[key] === undefined) process.env[key] = value;
    }
  } catch {
    /* no .env: fall back to the process environment */
  }
}
loadDotenv();

// When set, run the playground's own generated example sources (the literal
// playground diagnosis) instead of the curated battery.
const USE_DIAGNOSIS = process.env.E2E_DIAGNOSIS === "1";

// Signer-critical, chain-node-independent methods that must pass in diagnosis
// mode. Everything else in the full diagnosis either needs a live chain node
// (all Chain/*, live-chain transaction assembly) or is a deferred feature.
const MUST_PASS = new Set([
  "account.requestLogin",
  "Account/request_login",
  "Account/connection_status_subscribe",
  "Account/get_account",
  "Account/get_legacy_accounts",
  "Signing/sign_raw",
  "Signing/sign_payload",
  "Signing/sign_raw_with_legacy_account",
  "Signing/sign_payload_with_legacy_account",
  "Resource Allocation/request",
  "Statement Store/create_proof",
  "Statement Store/create_proof_authorized",
  "Statement Store/submit",
  "Entropy/derive",
]);

const REPO_ROOT = resolve(import.meta.dir, "../../../..");
const BINARY = resolve(REPO_ROOT, "target/debug/truapi-host");

/** A spawned host process whose stdout lines can be awaited by prefix. */
class HostProcess {
  private readonly lines: string[] = [];
  private readonly waiters: Array<{ prefix: string; resolve: (line: string) => void }> = [];
  readonly proc: ReturnType<typeof Bun.spawn>;

  constructor(label: string, args: string[]) {
    this.proc = Bun.spawn([BINARY, ...args], {
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? "info" },
    });
    this.pump(label, this.proc.stdout, false);
    this.pump(label, this.proc.stderr, true);
  }

  private async pump(label: string, stream: ReadableStream<Uint8Array>, isErr: boolean) {
    const decoder = new TextDecoder();
    let buffer = "";
    for await (const chunk of stream) {
      buffer += decoder.decode(chunk, { stream: true });
      let index: number;
      while ((index = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, index);
        buffer = buffer.slice(index + 1);
        if (isErr) {
          if (process.env.E2E_VERBOSE) console.error(`[${label}:err] ${line}`);
          continue;
        }
        console.error(`[${label}] ${line}`);
        this.lines.push(line);
        for (let i = this.waiters.length - 1; i >= 0; i--) {
          if (line.startsWith(this.waiters[i].prefix)) {
            this.waiters.splice(i, 1)[0].resolve(line);
          }
        }
      }
    }
  }

  waitFor(prefix: string, timeoutMs = 30_000): Promise<string> {
    const existing = this.lines.find((line) => line.startsWith(prefix));
    if (existing) return Promise.resolve(existing);
    return new Promise((resolvePromise, rejectPromise) => {
      const timer = setTimeout(
        () => rejectPromise(new Error(`timed out waiting for "${prefix}"`)),
        timeoutMs,
      );
      this.waiters.push({
        prefix,
        resolve: (line) => {
          clearTimeout(timer);
          resolvePromise(line);
        },
      });
    });
  }

  kill() {
    this.proc.kill();
  }
}

function wsUrlFrom(line: string, prefix: string): string {
  return line.slice(prefix.length).trim();
}

async function main() {
  if (!(await Bun.file(BINARY).exists())) {
    console.error(`missing binary ${BINARY}; run: cargo build -p truapi-host-cli`);
    process.exit(2);
  }

  const processes: HostProcess[] = [];
  const cleanup = () => processes.forEach((p) => p.kill());

  try {
    const relay = new HostProcess("relay", ["relay", "--listen", "127.0.0.1:0"]);
    processes.push(relay);
    const relayUrl = wsUrlFrom(await relay.waitFor("RELAY_LISTENING "), "RELAY_LISTENING ");

    // With live chain on, resolve usernames from the real People chain so
    // get_user_id works (SSO still runs over the relay).
    const liveChain = process.env.E2E_LIVE_CHAIN === "1";
    const pairing = new HostProcess("pairing", [
      "pairing-host",
      "--relay",
      relayUrl,
      "--frame-listen",
      "127.0.0.1:0",
      ...(liveChain ? ["--resolve-identity"] : []),
    ]);
    processes.push(pairing);
    const frameUrl = wsUrlFrom(await pairing.waitFor("FRAMES_LISTENING "), "FRAMES_LISTENING ");

    console.error(`connecting product client to ${frameUrl}`);
    const { client, opened, dispose } = connect(frameUrl);
    await opened;

    // Start login without awaiting: the pairing host emits the deeplink, which
    // we hand to the signing host; login resolves once pairing completes.
    const loginPromise = beginLogin(client);

    const deeplink = wsUrlFrom(
      await pairing.waitFor("PAIRING_DEEPLINK "),
      "PAIRING_DEEPLINK ",
    );

    // External-signer mode: hand the relay URL + deeplink to a separate
    // signing-host process (e.g. a second tmux pane) via a file, instead of
    // spawning the signer here. Lets the two hosts run as visibly separate
    // processes.
    const handoffFile = process.env.E2E_HANDOFF_FILE;
    if (handoffFile) {
      // Two lines: relay URL, then deeplink — trivially read by a shell script.
      await Bun.write(handoffFile, `${relayUrl}\n${deeplink}\n`);
      console.error(`wrote relay + deeplink handoff to ${handoffFile}; waiting for external signer`);
    } else {
      console.error(`launching signing host for deeplink ${deeplink.slice(0, 48)}...`);
      const mnemonic = process.env.E2E_SIGNER_MNEMONIC;
      const signing = new HostProcess("signing", [
        "signing-host",
        "--relay",
        relayUrl,
        "--deeplink",
        deeplink,
        ...(mnemonic ? ["--mnemonic", mnemonic] : []),
      ]);
      processes.push(signing);
      await signing.waitFor("SIGNING_HOST_READY");
    }

    const login = await loginPromise;
    const loginOk = login.isOk() && login.value === "Success";
    console.error(`login result: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
    if (!loginOk) throw new Error("pairing/login did not succeed");

    if (USE_DIAGNOSIS) {
      const rows = await runDiagnosis(client);
      dispose();
      const report = renderReport(rows);
      const path = resolve(REPO_ROOT, "explorer/diagnosis-reports/headless.md");
      await Bun.write(path, report);
      // Print the full web.md-shape table so the pane carries the same detail.
      console.log("\n" + report);
      const pass = rows.filter((r) => r.status === "pass").length;
      const fail = rows.filter((r) => r.status === "fail").length;
      const skip = rows.filter((r) => r.status === "skipped").length;
      console.log(`\nwrote ${path}`);
      console.log(`${pass} passed, ${fail} failed, ${skip} skipped (of ${rows.length})`);
      cleanup();
      // Gate on the signer path: chain-node methods and deferred features
      // (ring-VRF alias, identity, live-chain transaction assembly) fail
      // against the hermetic statement-store relay; those are environmental,
      // not signing regressions.
      const critical = rows.filter((r) => MUST_PASS.has(r.id) && r.status === "fail");
      if (critical.length > 0) {
        console.log(`\nGATE FAILED: ${critical.map((r) => r.id).join(", ")}`);
      } else {
        console.log(`\nGATE PASSED: all signer-critical methods pass`);
      }
      process.exit(critical.length === 0 ? 0 : 1);
    }

    const results: CaseResult[] = [
      { name: "account.requestLogin", ok: true, detail: String(login.value) },
      ...(await runBattery(client)),
    ];
    dispose();
    printSummary(results);
    cleanup();
    const failures = results.filter((r) => !r.ok);
    console.log(
      failures.length > 0
        ? `\nGATE FAILED: ${failures.map((r) => r.name).join(", ")}`
        : `\nGATE PASSED: ${results.length} signer-critical cases`,
    );
    process.exit(failures.length === 0 ? 0 : 1);
  } catch (error) {
    console.error(`e2e failed: ${String(error)}`);
    cleanup();
    process.exit(1);
  }
}

/** Collapse whitespace runs so multi-line JSON fits one table cell. */
function cleanDetail(output: string): string {
  const collapsed = output.replace(/\s+/g, " ").trim();
  return collapsed.length > 300 ? collapsed.slice(0, 297) + "..." : collapsed;
}

// Emit a report in the same table shape as explorer/diagnosis-reports/web.md
// so the headless run can be diffed against the browser host directly.
// Details carry the captured example output for passes and the error for
// failures; skipped rows are blank.
function renderReport(rows: DiagnosisRow[]): string {
  const icon = (s: DiagnosisRow["status"]) =>
    s === "pass" ? "✅" : s === "skipped" ? "⏭️" : "❌";
  return (
    [
      "## Truapi Headless Pairing Host Diagnosis",
      "",
      "| Method | Status | Details |",
      "| --- | --- | --- |",
      ...rows.map(
        (r) =>
          `| \`${r.id}\` | ${icon(r.status)} | ${r.status === "skipped" ? "" : cleanDetail(r.output)} |`,
      ),
    ].join("\n") + "\n"
  );
}

function printSummary(results: CaseResult[]) {
  const pass = results.filter((r) => r.ok).length;
  console.log("\n=== Headless host e2e results ===");
  for (const r of results) {
    console.log(`${r.ok ? "PASS" : "FAIL"}  ${r.name.padEnd(28)} ${r.detail}`);
  }
  console.log(`--------------------------------`);
  console.log(`${pass}/${results.length} passed`);
}

main();
