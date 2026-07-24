import { describe, expect, test } from "bun:test";
import { services } from "../../../../js/packages/truapi/src/playground/codegen/services.ts";
import { BatteryReporter, shouldUseColor } from "./battery-reporter.ts";
import {
  cliDiagnosisReportMetadata,
  renderDiagnosisReport,
} from "./diagnosis-report.ts";
import {
  createDiagnosisPlan,
  type DiagnosisCase,
  type DiagnosisRow,
} from "./diagnosis.ts";

describe("generated-example battery", () => {
  test("honors forced color for recorded non-TTY output", () => {
    expect(shouldUseColor(false, undefined, "1")).toBe(true);
    expect(shouldUseColor(false, undefined, "0")).toBe(false);
    expect(shouldUseColor(true, "1", "1")).toBe(false);
  });

  test("uses a distinct committed report for each CLI host role", () => {
    expect(cliDiagnosisReportMetadata("pairing-host")).toEqual({
      filename: "pairing-host-cli.md",
      title: "Truapi Pairing Host CLI Diagnosis",
    });
    expect(cliDiagnosisReportMetadata("signing-host")).toEqual({
      filename: "signing-host-cli.md",
      title: "Truapi Signing Host CLI Diagnosis",
    });
    expect(() => cliDiagnosisReportMetadata(undefined)).toThrow(
      "TRUAPI_CLI_HOST_ROLE must be pairing-host or signing-host",
    );
  });

  test("derives every case from the generated playground manifest", () => {
    const generatedIds = services.flatMap((service) =>
      service.methods.map((method) => `${service.name}/${method.name}`),
    );
    const plan = createDiagnosisPlan({ runKnownUnsupported: true });

    expect(plan.map((testCase) => testCase.id)).toEqual(generatedIds);
    expect(plan.every((testCase) => testCase.exampleSource)).toBe(true);
    expect(plan.every((testCase) => testCase.skipReason === undefined)).toBe(
      true,
    );
  });

  test("prints failures as concise test-reporter rows", () => {
    const output: string[] = [];
    const reporter = new BatteryReporter((line) => output.push(line), false);
    const plan: DiagnosisCase[] = [
      {
        id: "Account/get_user_id",
        serviceName: "Account",
        methodName: "get_user_id",
        exampleSource: "example",
      },
      {
        id: "Account/get_account_alias",
        serviceName: "Account",
        methodName: "get_account_alias",
        exampleSource: "example",
      },
    ];
    const rows: DiagnosisRow[] = [
      row(plan[0], "pass", "ok", 12),
      row(
        plan[1],
        "fail",
        "account alias: partial output\ngetAccountAlias failed: PermissionDenied",
        901,
      ),
    ];

    reporter.start(plan);
    reporter.paired("Success");
    reporter.waitingForHost(3_000);
    rows.forEach((result) => reporter.result(result));
    reporter.finish(rows, 1_200);
    reporter.reportSaved("/tmp/cli.md");

    const report = output.join("\n");
    expect(report).toContain("2 examples · 1 service · generated manifest");
    expect(report).toContain("✓ get_user_id");
    expect(report).toContain("× get_account_alias");
    expect(report).toContain("Waiting 3.0 s for signing-host readiness");
    expect(report).toContain("getAccountAlias failed: PermissionDenied");
    expect(report).toContain(
      "1 passed · 1 failed · 0 skipped · 2 total · 1.2 s",
    );
    expect(report).toContain("- Account/get_account_alias");
    expect(report).toContain("Report saved · /tmp/cli.md");
    expect(report).not.toContain("\u001b[");
  });

  test("renders the browser diagnosis Markdown shape", () => {
    const report = renderDiagnosisReport("Truapi CLI Diagnosis", [
      {
        id: "Account/get_user_id",
        serviceName: "Account",
        methodName: "get_user_id",
        status: "pass",
        output: "ok",
        durationMs: 12,
      },
      {
        id: "Chain/stop_transaction",
        serviceName: "Chain",
        methodName: "stop_transaction",
        status: "fail",
        output: "\u001b[31mInvalid operation | -32602\u001b[0m",
        durationMs: 901,
      },
    ]);

    expect(report).toBe(
      "## Truapi CLI Diagnosis\n\n" +
        "| Method | Status | Details |\n" +
        "| --- | --- | --- |\n" +
        "| `Account/get_user_id` | ✅ |  |\n" +
        "| `Chain/stop_transaction` | ❌ | Invalid operation \\| -32602 |\n",
    );
  });

  test("keeps the root cause when failure setup output is long", () => {
    const setup = `submitting statement: ${"a".repeat(400)}`;
    const rootCause = "subscription failed: statement expired";
    const output = `${setup}\n${rootCause}`;
    const lines: string[] = [];

    new BatteryReporter((line) => lines.push(line), false).result({
      id: "Statement Store/subscribe",
      serviceName: "Statement Store",
      methodName: "subscribe",
      status: "fail",
      output,
      durationMs: 10,
    });
    const report = renderDiagnosisReport("Truapi CLI Diagnosis", [
      {
        id: "Statement Store/subscribe",
        serviceName: "Statement Store",
        methodName: "subscribe",
        status: "fail",
        output,
        durationMs: 10,
      },
    ]);

    expect(lines.join("\n")).toContain(rootCause);
    expect(report).toContain(rootCause);
  });
});

function row(
  testCase: DiagnosisCase,
  status: DiagnosisRow["status"],
  output: string,
  durationMs: number,
): DiagnosisRow {
  return {
    id: testCase.id,
    serviceName: testCase.serviceName,
    methodName: testCase.methodName,
    status,
    output,
    durationMs,
  };
}
