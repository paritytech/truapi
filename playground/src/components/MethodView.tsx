"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { stringify } from "@/src/lib/host-api-bridge";
import { ExampleEditor } from "@/src/components/ExampleEditor";
import { runExample, type LogEntry } from "@/src/lib/example-runner";
import { getClientOrThrow } from "@parity/truapi/sandbox";
import { methodTestId, revealInRail, serviceTestId } from "@/src/lib/rail";
import { services } from "@/src/lib/services";
import type { MethodInfo, ServiceInfo } from "@/src/lib/services";

const CALL_TIMEOUT_MS = 30_000;

const CARGO_DOC_BASE =
  process.env.NEXT_PUBLIC_CARGO_DOC_BASE ??
  "https://paritytech.github.io/truapi/cargo_doc";

/** Deployed playground served inside the Polkadot Desktop Host. */
const HOSTED_PLAYGROUND_URL = "https://truapi-playground.dot.li";

function cargoDocMethodUrl(docUrl: string | undefined): string | undefined {
  return docUrl ? `${CARGO_DOC_BASE}/${docUrl}` : undefined;
}

/** Deep link that opens this method in the host-backed playground. */
function hostedPlaygroundUrl(service: string, method: string): string {
  const params = new URLSearchParams({ service, method });
  return `${HOSTED_PLAYGROUND_URL}/?${params.toString()}`;
}

/** Thrown by the transport when no host is detected (standalone tab). */
function isHostMissingError(error: string): boolean {
  return error.includes("must be opened inside a TrUAPI host");
}

function formatError(value: unknown): string {
  if (value instanceof Error) {
    const message = value.message || value.name || "Error";
    const payload = (value as Error & { payload?: unknown }).payload;
    if (payload && typeof payload === "object") {
      const payloadStr = stringify(payload);
      if (payloadStr === stringify({ reason: message })) return message;
      return `${message}\n\n${payloadStr}`;
    }
    return message;
  }
  if (typeof value === "string") return value;
  return stringify(value);
}

export function MethodView({
  service,
  method,
  onBack,
}: {
  service: string;
  method: string;
  onBack: () => void;
}) {
  const methodInfo = services
    .find((s: ServiceInfo) => s.name === service)
    ?.methods.find((m: MethodInfo) => m.name === method);

  const [source, setSource] = useState(methodInfo?.exampleSource ?? "");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const [tab, setTab] = useState<"example" | "output">("example");
  const [copied, setCopied] = useState(false);
  const callAbortRef = useRef<((reason: string) => void) | null>(null);
  const cancelRunRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    setSource(methodInfo?.exampleSource ?? "");
    setLogs([]);
    setError("");
    setRunning(false);
    callAbortRef.current?.("method changed");
    callAbortRef.current = null;
    cancelRunRef.current?.();
    cancelRunRef.current = null;
    setTab("example");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [service, method]);

  useEffect(
    () => () => {
      cancelRunRef.current?.();
      cancelRunRef.current = null;
    },
    [],
  );

  const onLog = useCallback((entry: LogEntry) => {
    setLogs((prev) => [...prev, entry]);
  }, []);

  // Scroll the index (left rail on desktop) to this method's service section.
  const revealServiceInRail = useCallback(() => {
    revealInRail(serviceTestId(service), { block: "start", smooth: true });
  }, [service]);

  // Scroll the index to this exact method row and select it.
  const revealMethodInRail = useCallback(() => {
    revealInRail(methodTestId(service, method), {
      block: "center",
      smooth: true,
      focus: true,
    });
  }, [service, method]);

  const runnable = !!methodInfo?.exampleSource;

  // Failure is explicit: the example resolves on success and throws (via
  // `assert(...)`, a timeout, or any uncaught error) on failure. `console.*`
  // output is captured into `logs` for display but never decides pass/fail.
  const handleRun = async () => {
    if (!runnable || !methodInfo) return;
    setRunning(true);
    setError("");
    setLogs([]);
    setTab("output");
    try {
      const run = await runExample({
        source,
        client: getClientOrThrow(),
        onLog,
      });
      cancelRunRef.current = run.cancel;
      let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
      const abortPromise = new Promise<never>((_, reject) => {
        callAbortRef.current = (reason: string) => reject(new Error(reason));
        timeoutHandle = setTimeout(
          () =>
            reject(new Error(`Call timed out after ${CALL_TIMEOUT_MS / 1000}s`)),
          CALL_TIMEOUT_MS,
        );
      });
      try {
        await Promise.race([run.promise, abortPromise]);
      } finally {
        if (timeoutHandle !== null) clearTimeout(timeoutHandle);
        callAbortRef.current = null;
        run.cancel();
        cancelRunRef.current = null;
        setRunning(false);
      }
    } catch (err) {
      setError(formatError(err));
      setRunning(false);
    }
  };

  const handleStop = () => {
    callAbortRef.current?.("Call aborted");
  };

  const handleCopyLogs = async () => {
    const text = [...logs.map((entry) => entry.text), ...(error ? [error] : [])]
      .join("\n")
      .trim();
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  };

  const kind = methodInfo?.type ?? "unary";

  const status: Status = error
    ? "error"
    : running
      ? "running"
      : logs.length > 0
        ? "success"
        : "idle";

  return (
    <div>
      <div className="view__top">
        <button
          type="button"
          className="back"
          data-testid="back-button"
          onClick={onBack}
          aria-label="Back to service list"
        >
          ← Index
        </button>
      </div>

      <button
        type="button"
        className="view__breadcrumb view__breadcrumb--link"
        onClick={revealServiceInRail}
        title="Show this service in the index"
      >
        {service}
      </button>
      <h1 className="view__title">
        <button
          type="button"
          className="view__title-link"
          onClick={revealMethodInRail}
          title="Show this method in the index"
        >
          <span className="view__slash">/</span>
          <span className="view__method">{method}</span>
        </button>
      </h1>
      <div className="view__kind" data-kind={kind}>
        {kind === "subscription" ? "Subscription" : "Request / Response"}
      </div>

      {(methodInfo?.signature || methodInfo?.description) && (
        <div className="panel">
          <div className="panel__head">
            <span className="panel__label">
              {methodInfo.description ? "Description" : "API"}
            </span>
          </div>
          {methodInfo.description && (
            <p className="panel__desc">{methodInfo.description}</p>
          )}
          {methodInfo.signature &&
            (() => {
              const href = cargoDocMethodUrl(methodInfo.docUrl);
              const content = methodInfo.signature;
              return href ? (
                <a
                  className="signature"
                  href={href}
                  target="_blank"
                  rel="noreferrer"
                  title="Open this method's full Rust definition in the cargo doc"
                >
                  {content}
                </a>
              ) : (
                <pre className="signature">{content}</pre>
              );
            })()}
        </div>
      )}

      <div className="panel panel--workspace">
        <div className="tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={tab === "example"}
            className={`tab${tab === "example" ? " tab--active" : ""}`}
            onClick={() => setTab("example")}
          >
            Example
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === "output"}
            className={`tab${tab === "output" ? " tab--active" : ""}`}
            onClick={() => setTab("output")}
          >
            Output
            <span
              className="tab__led"
              data-status={status}
              aria-hidden
              title={LED_LABEL[status]}
            />
          </button>
          <span className="tabs__filler" />
          <span className="tabs__lang">TypeScript</span>
        </div>

        {tab === "example" ? (
          <>
            {methodInfo?.exampleSource ? (
              <ExampleEditor
                source={source}
                onChange={setSource}
                uri={`file:///playground/${service}-${method}.ts`}
              />
            ) : (
              <div className="panel__hint">
                This method has no runnable example yet.
              </div>
            )}
            <div className="actions">
              {!runnable ? (
                <button type="button" className="btn btn--primary" disabled>
                  Not supported
                </button>
              ) : running ? (
                <button
                  type="button"
                  className="btn btn--stop"
                  data-testid="stop-button"
                  onClick={handleStop}
                >
                  <span className="btn__glyph">■</span>
                  Stop (running…)
                </button>
              ) : (
                <button
                  type="button"
                  className="btn btn--primary"
                  data-testid={
                    kind === "subscription" ? "subscribe-button" : "call-button"
                  }
                  onClick={handleRun}
                >
                  <span className="btn__glyph">
                    {kind === "subscription" ? "●" : "→"}
                  </span>
                  Run example
                </button>
              )}
            </div>
          </>
        ) : (
          <div className="console console--inline" data-status={status}>
            {logs.length > 0 || error ? (
              <>
                <button
                  type="button"
                  className="console__copy"
                  data-copied={copied}
                  onClick={handleCopyLogs}
                  aria-label="Copy output to clipboard"
                  title="Copy output"
                >
                  {copied ? (
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden
                    >
                      <path d="M20 6 9 17l-5-5" />
                    </svg>
                  ) : (
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden
                    >
                      <rect x="8" y="8" width="14" height="14" rx="2" ry="2" />
                      <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
                    </svg>
                  )}
                </button>
                {logs.length > 0 && (
                  <div className="console__body" data-testid="stream-log">
                    {logs.map((entry, i) => (
                      <div
                        key={i}
                        className={`console__entry console__entry--${entry.level}`}
                        data-testid="stream-entry"
                      >
                        <span className="console__entry-i">
                          {String(i + 1).padStart(2, "0")}
                        </span>
                        <span className="console__entry-body">{entry.text}</span>
                      </div>
                    ))}
                  </div>
                )}
                {error && (
                  <div
                    className="console__body console__body--error"
                    data-testid="error-display"
                  >
                    {error}
                    {isHostMissingError(error) && (
                      <div className="console__cta">
                        <a
                          className="open-in-dotli"
                          href={hostedPlaygroundUrl(service, method)}
                          target="_blank"
                          rel="noreferrer"
                          title="Open this example in the host-backed playground"
                        >
                          Run in hosted playground ↗
                        </a>
                      </div>
                    )}
                  </div>
                )}
              </>
            ) : (
              <div className="console__body console__body--empty">
                {!runnable
                  ? "This method has no runnable example yet."
                  : status === "running"
                    ? "Waiting for response…"
                    : "Run the example to see output here."}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

type Status = "idle" | "running" | "success" | "error";

const LED_LABEL: Record<Status, string> = {
  idle: "Idle",
  running: "Running",
  success: "Success",
  error: "Error",
};
