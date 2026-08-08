/// <reference path="../runner.ts" />
// Runs every example emitted by TrUAPI codegen for the playground. The battery
// contains no hand-maintained method list: adding an exposed API example to the
// generated service manifest automatically adds it to this suite.
//
// Run via:
//   truapi-host pairing-host --product-id truapi-playground.dot \
//     --auto-accept --script js/scripts/battery.ts

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { runAutoSigningE2e } from "../auto-signing-e2e.ts";
import { BatteryReporter } from "../battery-reporter.ts";
import {
  cliDiagnosisReportMetadata,
  renderDiagnosisReport,
} from "../diagnosis-report.ts";
import {
  createDiagnosisPlan,
  expectedCliBatteryFailureReason,
  runDiagnosis,
} from "../diagnosis.ts";

const report = cliDiagnosisReportMetadata(process.env.TRUAPI_CLI_HOST_ROLE);
const DEFAULT_REPORT_PATH = fileURLToPath(
  new URL(
    `../../../../../explorer/diagnosis-reports/spa/${report.filename}`,
    import.meta.url,
  ),
);
const REPORT_PATH =
  process.env.TRUAPI_BATTERY_REPORT_PATH || DEFAULT_REPORT_PATH;
const options = { runKnownUnsupported: true } as const;
const plan = createDiagnosisPlan(options);
const reporter = new BatteryReporter();
reporter.start(plan);

const login = await truapi.account.requestLogin({ reason: undefined });
if (
  !login.isOk() ||
  !["Success", "AlreadyConnected"].includes(String(login.value))
) {
  throw new Error(
    `battery pairing failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`,
  );
}
reporter.paired(login.value);

// Pairing completes before the signing host has necessarily finished preparing
// its wallet/ring state and started its SSO responder. Give that responder a
// small readiness window so the first remote example cannot race startup.
const HOST_READINESS_DELAY_MS = 3_000;
reporter.waitingForHost(HOST_READINESS_DELAY_MS);
await new Promise((resolve) => setTimeout(resolve, HOST_READINESS_DELAY_MS));

const startedAt = performance.now();
const rows = await runDiagnosis(truapi, {
  ...options,
  onResult: (row) => reporter.result(row),
});
// Beyond the generated per-method examples: AutoSigning must make follow-up
// sign_vrf calls prompt-free, observed through the host's approvals transcript.
const autoSigning = await runAutoSigningE2e(
  truapi,
  host.productId,
  process.env.TRUAPI_APPROVALS_LOG,
);
reporter.result(autoSigning);
rows.push(autoSigning);
reporter.finish(rows, Math.round(performance.now() - startedAt));
mkdirSync(dirname(REPORT_PATH), { recursive: true });
writeFileSync(REPORT_PATH, renderDiagnosisReport(report.title, rows));
reporter.reportSaved(REPORT_PATH);

const failures = rows.filter((row) => row.status === "fail");
const unexpectedFailures = failures.filter(
  (row) => !expectedCliBatteryFailureReason(row.serviceName),
);
if (unexpectedFailures.length > 0) {
  throw new Error(
    `TrUAPI battery failed: ${unexpectedFailures.length} of ${rows.length} generated examples failed outside the known unsupported baseline`,
  );
}
if (failures.length > 0) {
  console.log(
    `Known unsupported baseline: ${failures.length} generated examples failed as expected`,
  );
}
