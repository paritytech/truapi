"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { stringify } from "@/src/lib/host-api-bridge";
import { ExampleEditor } from "@/src/components/ExampleEditor";
import {
  runExample,
  type LogEntry,
  type RunSubscription,
} from "@/src/lib/example-runner";
import { getClient } from "@/src/lib/transport";
import { errorTextFrom } from "@/src/lib/result-status";
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
  const [result, setResult] = useState("");
  const [activeSub, setActiveSub] = useState<RunSubscription | null>(null);
  const [tab, setTab] = useState<"example" | "output">("example");
  const callAbortRef = useRef<((reason: string) => void) | null>(null);
  const cancelRunRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    setSource(methodInfo?.exampleSource ?? "");
    setLogs([]);
    setError("");
    setResult("");
    setRunning(false);
    setActiveSub((prev) => {
      try {
        prev?.unsubscribe();
      } catch {
        /* benign */
      }
      return null;
    });
    callAbortRef.current?.("method changed");
    callAbortRef.current = null;
    cancelRunRef.current?.();
    cancelRunRef.current = null;
    setTab("example");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [service, method]);

  useEffect(
    () => () => {
      try {
        activeSub?.unsubscribe();
      } catch {
        /* benign */
      }
    },
    [activeSub],
  );

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

  const runnable = !!methodInfo?.exampleSource;

  const handleRun = async () => {
    if (!runnable || !methodInfo) return;
    setRunning(true);
    setError("");
    setResult("");
    setLogs([]);
    setTab("output");
    // Examples self-handle their Result (`result.match(v => console.log(v),
    // e => console.error(e))`), so an Err surfaces as an error-level log rather
    // than a thrown exception. Accumulate logs locally — the `logs` React state
    // is stale inside this handler — so we can detect the error after the call.
    const callLogs: LogEntry[] = [];
    const collectLog = (entry: LogEntry) => {
      callLogs.push(entry);
      onLog(entry);
    };
    try {
      const client = getClient();
      const run = await runExample({
        source,
        kind: methodInfo.type,
        client,
        onLog: collectLog,
      });

      if (run.kind === "unary") {
        cancelRunRef.current = run.cancel;
        let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
        const abortPromise = new Promise<never>((_, reject) => {
          callAbortRef.current = (reason: string) => reject(new Error(reason));
          timeoutHandle = setTimeout(
            () =>
              reject(
                new Error(`Call timed out after ${CALL_TIMEOUT_MS / 1000}s`),
              ),
            CALL_TIMEOUT_MS,
          );
        });
        try {
          const value = await Promise.race([run.promise, abortPromise]);
          const errText = errorTextFrom(value, callLogs);
          if (errText != null) {
            setError(errText);
          } else {
            const rendered = stringify(value);
            if (rendered !== undefined) setResult(rendered);
          }
        } finally {
          if (timeoutHandle !== null) clearTimeout(timeoutHandle);
          callAbortRef.current = null;
          cancelRunRef.current = null;
          setRunning(false);
        }
      } else {
        setActiveSub(run.subscription);
      }
    } catch (err) {
      setError(formatError(err));
      setRunning(false);
    }
  };

  const handleStop = () => {
    if (callAbortRef.current) {
      callAbortRef.current("Call aborted");
      return;
    }
    if (activeSub) {
      try {
        activeSub.unsubscribe();
      } catch {
        /* benign */
      }
      setActiveSub(null);
      setRunning(false);
      setLogs((prev) => [...prev, { level: "log", text: "--- stopped ---" }]);
    }
  };

  const kind = methodInfo?.type ?? "unary";

  const status: Status = error
    ? "error"
    : activeSub
      ? "streaming"
      : running
        ? "running"
        : result
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

      <div className="view__breadcrumb">{service}</div>
      <h1 className="view__title">
        <span className="view__slash">/</span>
        <span className="view__method">{method}</span>
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
              ) : kind === "subscription" ? (
                activeSub ? (
                  <button
                    type="button"
                    className="btn btn--stop"
                    data-testid="stop-button"
                    onClick={handleStop}
                  >
                    <span className="btn__glyph">■</span>
                    Stop
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn btn--primary"
                    data-testid="subscribe-button"
                    onClick={handleRun}
                  >
                    <span className="btn__glyph">●</span>
                    Run example
                  </button>
                )
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
                  data-testid="call-button"
                  onClick={handleRun}
                >
                  <span className="btn__glyph">→</span>
                  Run example
                </button>
              )}
            </div>
          </>
        ) : (
          <div className="console console--inline" data-status={status}>
            {error ? (
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
            ) : result ? (
              <div className="console__body" data-testid="response-content">
                {result}
              </div>
            ) : logs.length > 0 ? (
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
            ) : (
              <div className="console__body console__body--empty">
                {!runnable
                  ? "This method has no runnable example yet."
                  : status === "running"
                    ? "Waiting for response…"
                    : status === "streaming"
                      ? "Waiting for first event…"
                      : "Run the example to see output here."}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

type Status = "idle" | "running" | "streaming" | "success" | "error";

const LED_LABEL: Record<Status, string> = {
  idle: "Idle",
  running: "Running",
  streaming: "Streaming",
  success: "Success",
  error: "Error",
};
