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
import { services } from "@/src/lib/services";
import type { MethodInfo, ServiceInfo } from "@/src/lib/services";

function renderWithLinks(text: string) {
  const parts = text.split(/(\[[^\]]+\]\("[^"]+"\))/g);
  return parts.map((part, i) => {
    const match = part.match(/^\[([^\]]+)\]\("([^"]+)"\)$/);
    if (match) {
      return (
        <a key={i} href={match[2]} target="_blank" rel="noreferrer">
          {match[1]}
        </a>
      );
    }
    return part;
  });
}

const CALL_TIMEOUT_MS = 30_000;

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
  const callAbortRef = useRef<((reason: string) => void) | null>(null);

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

  const onLog = useCallback((entry: LogEntry) => {
    setLogs((prev) => [...prev, entry]);
  }, []);

  const runnable =
    !!methodInfo?.exampleSource && !!methodInfo?.exampleFunctionName;

  const handleRun = async () => {
    if (!runnable || !methodInfo) return;
    setRunning(true);
    setError("");
    setResult("");
    setLogs([]);
    try {
      const client = getClient();
      const run = await runExample({
        source,
        functionName: methodInfo.exampleFunctionName!,
        client,
        onLog,
      });

      if (run.kind === "unary") {
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
          setResult(stringify(value) ?? "null");
        } finally {
          if (timeoutHandle !== null) clearTimeout(timeoutHandle);
          callAbortRef.current = null;
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

      {methodInfo?.description && (
        <div className="panel">
          <div className="panel__head">
            <span className="panel__label">Description</span>
          </div>
          <p className="panel__desc">{methodInfo.description}</p>
        </div>
      )}

      <div className="panel">
        {methodInfo?.exampleSource ? (
          <>
            <div className="panel__head">
              <span className="panel__label">Example</span>
              <span className="panel__label" style={{ color: "var(--ink-4)" }}>
                TypeScript
              </span>
            </div>
            {methodInfo.requestDescription && (
              <div className="panel__hint">
                {renderWithLinks(methodInfo.requestDescription)}
              </div>
            )}
            <ExampleEditor
              source={source}
              onChange={setSource}
              uri={`file:///playground/${service}-${method}.ts`}
            />
          </>
        ) : (
          <div className="panel__head">
            <span className="panel__label">No runnable example</span>
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
      </div>

      {(result || error || logs.length > 0) && (
        <div className="console">
          <div className="console__head">
            <span className="console__title">
              {error
                ? "Error"
                : kind === "subscription"
                  ? "Stream output"
                  : "Result"}
            </span>
            <span className="console__dots" aria-hidden>
              <i />
              <i />
              <i />
            </span>
          </div>
          {error && (
            <div
              className="console__body console__body--error"
              data-testid="error-display"
            >
              {error}
            </div>
          )}
          {result && (
            <div className="console__body" data-testid="response-content">
              {result}
            </div>
          )}
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
        </div>
      )}
    </div>
  );
}
