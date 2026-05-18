import { useState, useEffect, useMemo, useRef } from "react";
import type { MethodInfo, ServiceInfo } from "@/src/lib/services";
import type { TestEntry, TestStatus } from "@/src/lib/auto-test";

const STATUS_LABEL: Record<TestStatus, string> = {
  idle: "—",
  running: "running…",
  pass: "pass",
  fail: "fail",
  skipped: "skip",
};

export function AutoTestView({
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
  onRun: (mode: "all" | "safe") => void;
  onStop: () => void;
  onRetry: (
    serviceName: string,
    methodName: string,
    requestOverride?: string,
  ) => void;
  onBack: () => void;
}) {
  const [mode, setMode] = useState<"all" | "safe">("safe");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [editedRequest, setEditedRequest] = useState<string>("");
  const editTextareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (isRunning) setExpandedId(null);
  }, [isRunning]);

  useEffect(() => {
    const el = editTextareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [editedRequest, expandedId]);

  const toggleExpand = (id: string) => {
    if (expandedId === id) {
      setExpandedId(null);
      return;
    }
    setExpandedId(id);
    setEditedRequest(testResults[id]?.request ?? "");
  };

  const { hasResults, passCount, failCount } = useMemo(() => {
    const entries = Object.values(testResults);
    return {
      hasResults: entries.length > 0,
      passCount: entries.filter((e) => e.status === "pass").length,
      failCount: entries.filter((e) => e.status === "fail").length,
    };
  }, [testResults]);

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
      <h1 className="view__title">
        Auto<span className="view__slash">-</span>Test
      </h1>

      <div className="panel">
        <div className="panel__head">
          <span className="panel__label">About</span>
        </div>
        <p className="panel__desc">
          Calls every supported method with its default request and reports pass
          or fail. Disruptive methods (those that open pop-ups or navigate away)
          can be skipped with the toggle below.
        </p>
      </div>

      <div className="autotest__controls">
        <div className="autotest__mode" aria-label="Test scope">
          <button
            type="button"
            className="autotest__mode-btn"
            data-active={mode === "safe"}
            disabled={isRunning}
            onClick={() => setMode("safe")}
          >
            Skip disruptive
          </button>
          <button
            type="button"
            className="autotest__mode-btn"
            data-active={mode === "all"}
            disabled={isRunning}
            onClick={() => setMode("all")}
          >
            All methods
          </button>
        </div>

        <div className="actions">
          {isRunning ? (
            <button type="button" className="btn btn--stop" onClick={onStop}>
              <span className="btn__glyph">■</span>
              Stop
            </button>
          ) : (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => onRun(mode)}
            >
              <span className="btn__glyph">▶</span>
              Run All Tests
            </button>
          )}
          {hasResults && (
            <span
              className="autotest__summary"
              data-has-fail={!isRunning && failCount > 0}
            >
              {passCount} pass · {failCount} fail
            </span>
          )}
        </div>
      </div>

      {hasResults && (
        <div className="autotest">
          {services.map((svc) => (
            <div key={svc.name} className="autotest__group">
              <div className="autotest__group-head">{svc.name}</div>
              {svc.methods.map((m: MethodInfo) => {
                const id = `${svc.name}/${m.name}`;
                const entry = testResults[id];
                const status = entry?.status ?? "idle";
                const isExpandable = status === "pass" || status === "fail";
                const isExpanded = expandedId === id;

                return (
                  <div key={m.name}>
                    <div
                      className="autotest__row"
                      data-status={status}
                      data-expandable={isExpandable}
                      onClick={
                        isExpandable ? () => toggleExpand(id) : undefined
                      }
                    >
                      <span className="autotest__dot" data-status={status} />
                      <span className="autotest__name">{m.name}</span>
                      {isExpandable && (
                        <span className="autotest__chevron">
                          {isExpanded ? "▲" : "▼"}
                        </span>
                      )}
                      <span className="autotest__status" data-status={status}>
                        {STATUS_LABEL[status]}
                      </span>
                    </div>
                    {isExpanded && (
                      <div
                        className="autotest__detail"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <div className="autotest__detail-label">
                          Request (editable)
                        </div>
                        <textarea
                          ref={editTextareaRef}
                          className="autotest__detail-edit"
                          value={editedRequest}
                          spellCheck={false}
                          autoCapitalize="off"
                          autoCorrect="off"
                          onChange={(e) => setEditedRequest(e.target.value)}
                        />
                        {entry?.output != null && (
                          <>
                            <div className="autotest__detail-label">
                              Response
                            </div>
                            <pre className="autotest__detail-body">
                              {entry.output}
                            </pre>
                          </>
                        )}
                        <button
                          type="button"
                          className="autotest__retry"
                          disabled={isRunning}
                          title={
                            isRunning
                              ? "Wait for the auto-test run to finish before retrying"
                              : undefined
                          }
                          onClick={(e) => {
                            e.stopPropagation();
                            const override =
                              editedRequest === entry?.request
                                ? undefined
                                : editedRequest;
                            onRetry(svc.name, m.name, override);
                          }}
                        >
                          Retry
                        </button>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
