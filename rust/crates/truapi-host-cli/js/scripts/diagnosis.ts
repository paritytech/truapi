/// <reference path="../runner.ts" />
// Full playground diagnosis, as a product script for the pairing host.
//
// Run via: truapi-host pairing-host --product-id truapi-playground.dot --script js/scripts/diagnosis.ts
// The generated example sources hardcode the `truapi-playground.dot` product, so
// the pairing host must serve that product id (else signing methods fail with
// PermissionDenied). Top-level product code: logs in, runs the examples against
// the paired signing host, writes a web.md-shape report to
// explorer/diagnosis-reports/headless-pairing.md, and gates on the signer-critical
// methods (chain-node methods and deferred features are reported, not gated,
// unless a live chain node is routed in).
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { runDiagnosis, type DiagnosisRow } from "../diagnosis.ts";

// Signer-critical, chain-node-independent methods that must pass.
const MUST_PASS = new Set([
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

const DEFAULT_REPORT_PATH = fileURLToPath(
  new URL("../../../../../explorer/diagnosis-reports/headless-pairing.md", import.meta.url),
);
const REPORT_PATH = process.env.TRUAPI_DIAGNOSIS_REPORT_PATH || DEFAULT_REPORT_PATH;
const REPORT_TITLE = process.env.TRUAPI_DIAGNOSIS_TITLE || "Truapi Headless Pairing Host Diagnosis";

const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || !["Success", "AlreadyConnected"].includes(String(login.value))) {
  throw new Error(`login failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
}

const rows = await runDiagnosis(truapi);
mkdirSync(dirname(REPORT_PATH), { recursive: true });
writeFileSync(REPORT_PATH, renderReport(rows));

console.log("\n" + renderReport(rows));
const pass = rows.filter((r) => r.status === "pass").length;
const fail = rows.filter((r) => r.status === "fail").length;
const skip = rows.filter((r) => r.status === "skipped").length;
console.log(`\nwrote ${REPORT_PATH}`);
console.log(`${pass} passed, ${fail} failed, ${skip} skipped (of ${rows.length})`);

const critical = rows.filter((r) => MUST_PASS.has(r.id) && r.status === "fail");
if (critical.length > 0) {
  throw new Error(`GATE FAILED: ${critical.map((r) => r.id).join(", ")}`);
}
console.log("GATE PASSED: all signer-critical methods pass");

// web.md-shape table so the headless run can be diffed against the browser host.
function renderReport(rows: DiagnosisRow[]): string {
  const icon = (s: DiagnosisRow["status"]) =>
      s === "pass" ? "✅" : s === "skipped" ? "⏭️" : "❌";
  return (
    [
      `## ${REPORT_TITLE}`,
      "",
      "| Method | Status | Details |",
      "| --- | --- | --- |",
      ...rows.map(
        (r) => `| \`${r.id}\` | ${icon(r.status)} | ${r.status === "fail" ? cleanDetail(r.output) : ""} |`,
      ),
    ].join("\n") + "\n"
  );
}

function cleanDetail(output: string): string {
  const collapsed = output.replace(/\s+/g, " ").trim();
  return collapsed.length > 300 ? collapsed.slice(0, 297) + "..." : collapsed;
}
