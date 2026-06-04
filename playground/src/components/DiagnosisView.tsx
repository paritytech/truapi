import { useMemo, useState } from "react";
import type { ServiceInfo } from "@/src/lib/services";
import type { TestEntry, TestStatus } from "@/src/lib/auto-test";
import {
  detectHostMode,
  renderReportMarkdown,
  reportIssueUrl,
} from "@/src/lib/diagnosis-report";
import { getClient } from "@/src/lib/transport";

const STATUS_LABEL: Record<TestStatus, string> = {
  idle: "queued",
  running: "processing…",
  pass: "success",
  fail: "failed",
  skipped: "skipped",
};

interface Row {
  id: string;
  service: string;
  method: string;
  status: TestStatus;
  output?: string;
}

export function DiagnosisView({
  services,
  testResults,
  isRunning,
  onRun,
  onStop,
  onRetry,
  onBack,
}: {
  services: ServiceInfo[];
  testResults: Record<string, TestEntry>;
  isRunning: boolean;
  onRun: () => void;
  onStop: () => void;
  onRetry: (service: string, method: string) => void;
  onBack: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const { rows, hasResults, passCount, failCount } = useMemo(() => {
    const out: Row[] = [];
    let pass = 0;
    let fail = 0;
    for (const svc of services) {
      for (const m of svc.methods) {
        const id = `${svc.name}/${m.name}`;
        const entry = testResults[id];
        const status = entry?.status ?? "idle";
        if (status === "pass") pass++;
        else if (status === "fail") fail++;
        out.push({
          id,
          service: svc.name,
          method: m.name,
          status,
          output: entry?.output,
        });
      }
    }
    return {
      rows: out,
      hasResults: Object.keys(testResults).length > 0,
      passCount: pass,
      failCount: fail,
    };
  }, [services, testResults]);

  const handleCopyReport = async () => {
    try {
      // Rendered on demand: the full report is only needed on copy, not on
      // every per-method result update during a run.
      await navigator.clipboard.writeText(
        renderReportMarkdown(services, testResults),
      );
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  };

  // Open a pre-filled GitHub issue carrying the report; the diagnosis-report
  // workflow writes it to diagnosis-reports/<host>.md and opens a PR. The host
  // opens the link via `navigate_to` (a sandboxed app can't `window.open`).
  // Copy the report to the clipboard first as a fallback if the body is
  // truncated.
  const handleSubmitReport = () => {
    const report = renderReportMarkdown(services, testResults);
    void navigator.clipboard?.writeText(report).catch(() => {});
    const url = reportIssueUrl(report, detectHostMode());
    try {
      void getClient().system.navigateTo({ url });
    } catch {
      /* no host connection */
    }
  };

  return (
    <div>
      <div className="view__top">
        <button
          type="button"
          className="back"
          onClick={onBack}
          aria-label="Back to service list"
        >
          ← Index
        </button>
      </div>

      <div className="view__breadcrumb">Playground</div>
      <h1 className="view__title">Diagnosis</h1>

      <div className="panel">
        <div className="panel__head">
          <span className="panel__label">About</span>
        </div>
        <p className="panel__desc">
          Runs every TrUAPI method against the connected host to build a coverage
          report — which methods work, which fail, and which aren&apos;t wired
          yet. Methods run one at a time, in order; those that need your approval
          (signing, permission and resource requests) wait on your response
          before the run continues. When it finishes, copy the report below.
        </p>
        <p className="diag__callout">
          Before you start: make sure you are <strong>logged in</strong>, and
          keep your <strong>phone nearby</strong> to sign transactions and
          approve pop-ups from the Polkadot app as they appear. Some payment
          methods need an <strong>available balance</strong> in the Polkadot app
          — without it they fail with an insufficient-balance error.
        </p>
      </div>

      <div className="actions">
        {isRunning ? (
          <button type="button" className="btn btn--stop" onClick={onStop}>
            <span className="btn__glyph">■</span>
            Stop
          </button>
        ) : (
          <button type="button" className="btn btn--primary" onClick={onRun}>
            <span className="btn__glyph">▶</span>
            Run diagnosis
          </button>
        )}
        {hasResults && (
          <span
            className="autotest__summary"
            data-has-fail={!isRunning && failCount > 0}
          >
            {passCount} success · {failCount} failed
          </span>
        )}
        {hasResults && !isRunning && (
          <div className="diag__report-actions">
            <button
              type="button"
              className="autotest__report-copy"
              onClick={handleCopyReport}
            >
              {copied ? "Copied ✓" : "Copy report"}
            </button>
            <button
              type="button"
              className="autotest__report-copy diag__submit"
              onClick={handleSubmitReport}
              title="Open a pre-filled GitHub issue that files this report as a PR"
            >
              Submit report ↗
            </button>
          </div>
        )}
      </div>

      {hasResults && (
        <div className="diag__log">
          {rows.map((r) => {
            const expandable = r.output != null;
            const isExpanded = expandedId === r.id;
            return (
              <div key={r.id}>
                <div
                  className="diag__row"
                  data-status={r.status}
                  data-expandable={expandable}
                  onClick={
                    expandable
                      ? () => setExpandedId(isExpanded ? null : r.id)
                      : undefined
                  }
                >
                  <span className="autotest__dot" data-status={r.status} />
                  <span className="diag__name">{r.id}</span>
                  {expandable && (
                    <span className="autotest__chevron">
                      {isExpanded ? "▲" : "▼"}
                    </span>
                  )}
                  <span className="autotest__status" data-status={r.status}>
                    {STATUS_LABEL[r.status]}
                  </span>
                </div>
                {isExpanded && r.output != null && (
                  <div className="autotest__detail">
                    <div className="autotest__detail-label">
                      {r.status === "fail" ? "Error" : "Response"}
                    </div>
                    <pre className="autotest__detail-body">{r.output}</pre>
                    <button
                      type="button"
                      className="autotest__retry"
                      disabled={isRunning}
                      title={
                        isRunning
                          ? "Wait for the diagnosis run to finish before replaying"
                          : "Re-run this method"
                      }
                      onClick={() => onRetry(r.service, r.method)}
                    >
                      ▶ Replay
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
