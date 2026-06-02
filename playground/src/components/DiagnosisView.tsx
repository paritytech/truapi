import { useMemo, useState } from "react";
import type { ServiceInfo } from "@/src/lib/services";
import type { TestEntry, TestStatus } from "@/src/lib/auto-test";
import { renderReportMarkdown } from "@/src/lib/diagnosis-report";

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

  const rows: Row[] = useMemo(() => {
    const out: Row[] = [];
    for (const svc of services) {
      for (const m of svc.methods) {
        const id = `${svc.name}/${m.name}`;
        const entry = testResults[id];
        out.push({
          id,
          service: svc.name,
          method: m.name,
          status: entry?.status ?? "idle",
          output: entry?.output,
        });
      }
    }
    return out;
  }, [services, testResults]);

  const { hasResults, passCount, failCount } = useMemo(() => {
    const entries = Object.values(testResults);
    return {
      hasResults: entries.length > 0,
      passCount: entries.filter((e) => e.status === "pass").length,
      failCount: entries.filter((e) => e.status === "fail").length,
    };
  }, [testResults]);

  const reportMarkdown = useMemo(
    () => renderReportMarkdown(services, testResults),
    [services, testResults],
  );

  const handleCopyReport = async () => {
    try {
      await navigator.clipboard.writeText(reportMarkdown);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
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
          yet. Non-disruptive methods run first in parallel, then methods that
          need your approval (signing, permission and resource requests) run one
          at a time. When it finishes, copy the report below.
        </p>
        <p className="diag__callout">
          Before you start: make sure you are <strong>logged in</strong>, and
          keep your <strong>phone nearby</strong> to sign transactions and
          approve pop-ups from the Polkadot app as they appear.
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
          <button
            type="button"
            className="autotest__report-copy"
            onClick={handleCopyReport}
          >
            {copied ? "Copied ✓" : "Copy report"}
          </button>
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
