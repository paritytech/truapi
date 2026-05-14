import { useState, useEffect, useRef } from "react";
import { getMethodBinding, stringify } from "@/src/lib/host-api-bridge";
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

// SDK errors are Error subclasses with a `payload` field. JSON.stringify drops
// `message` (non-enumerable) and emits the redundant instance/name/payload trio.
// Surface the message and append the payload only when it adds information.
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
  const noParams = methodInfo?.noParams ?? false;

  const formatDefault = (raw: string) => {
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  };

  const buildInitialRequest = () =>
    formatDefault(methodInfo?.defaultRequest ?? "{}");

  const [request, setRequest] = useState(buildInitialRequest);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [request]);

  useEffect(() => {
    setRequest(buildInitialRequest());
    setResponse("");
    setError("");
    setStreamLog([]);
    setStreamActive(false);
    setActiveSub((prev) => {
      prev?.unsubscribe();
      return null;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [service, method]);

  const [response, setResponse] = useState<string>("");
  const [error, setError] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [streamLog, setStreamLog] = useState<string[]>([]);
  const [streamActive, setStreamActive] = useState(false);
  const [activeSub, setActiveSub] = useState<{
    unsubscribe: () => void;
  } | null>(null);

  useEffect(() => {
    return () => {
      activeSub?.unsubscribe();
    };
  }, [activeSub]);
  const callAbortRef = useRef<((reason: string) => void) | null>(null);

  const CALL_TIMEOUT_MS = 30_000;

  const binding = getMethodBinding(service, method);

  const handleCall = async () => {
    if (!binding) return;
    setResponse("");
    setError("");
    setStreamLog([]);

    let parsed: unknown;
    if (noParams) {
      parsed = null;
    } else {
      try {
        parsed = JSON.parse(request);
      } catch {
        setError("Invalid JSON request");
        return;
      }
    }

    if (binding.isStream) {
      setStreamActive(true);
      const sub = binding.subscribe(
        parsed,
        (event) => {
          setStreamLog((prev) => [...prev, stringify(event)]);
        },
        (error) => {
          setStreamLog((prev) => [
            ...prev,
            error
              ? `--- stream error: ${error.message} ---`
              : "--- stream ended ---",
          ]);
          setStreamActive(false);
          setActiveSub(null);
        },
      );
      setActiveSub(sub);
    } else {
      setLoading(true);
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
        const result = await Promise.race([binding.call(parsed), abortPromise]);
        if (result.ok) {
          setResponse(stringify(result.data) ?? "null");
        } else {
          setError(formatError(result.data));
        }
      } catch (e: unknown) {
        setError(formatError(e));
      } finally {
        if (timeoutHandle !== null) clearTimeout(timeoutHandle);
        callAbortRef.current = null;
        setLoading(false);
      }
    }
  };

  const handleStop = () => {
    if (loading && callAbortRef.current) {
      callAbortRef.current("Call aborted");
      return;
    }
    activeSub?.unsubscribe();
    setStreamActive(false);
    setActiveSub(null);
    setStreamLog((prev) => [...prev, "--- stopped ---"]);
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

      {/* Description */}
      {methodInfo?.description && (
        <div className="panel">
          <div className="panel__head">
            <span className="panel__label">Description</span>
          </div>
          <p className="panel__desc">{methodInfo.description}</p>
        </div>
      )}

      <div className="panel">
        {!noParams && (
          <>
            <div className="panel__head">
              <span className="panel__label">Request Payload</span>
              <span className="panel__label" style={{ color: "var(--ink-4)" }}>
                JSON
              </span>
            </div>
            {methodInfo?.requestDescription && (
              <div className="panel__hint">
                {renderWithLinks(methodInfo.requestDescription)}
              </div>
            )}
            <div className="editor">
              <textarea
                ref={textareaRef}
                className="editor__area"
                data-testid="request-editor"
                value={request}
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                onChange={(e) => setRequest(e.target.value)}
              />
            </div>
          </>
        )}
        {noParams && (
          <div className="panel__head">
            <span className="panel__label">No Parameters</span>
          </div>
        )}
        <div className="actions">
          {!binding ? (
            <button type="button" className="btn btn--primary" disabled>
              Not supported
            </button>
          ) : binding.isStream ? (
            streamActive ? (
              <button
                type="button"
                className="btn btn--stop"
                data-testid="stop-button"
                onClick={handleStop}
              >
                <span className="btn__glyph">■</span>
                Stop stream
              </button>
            ) : (
              <button
                type="button"
                className="btn btn--primary"
                data-testid="subscribe-button"
                onClick={handleCall}
              >
                <span className="btn__glyph">●</span>
                Subscribe
              </button>
            )
          ) : loading ? (
            <button
              type="button"
              className="btn btn--stop"
              data-testid="stop-button"
              onClick={handleStop}
            >
              <span className="btn__glyph">■</span>
              Stop (calling…)
            </button>
          ) : (
            <button
              type="button"
              className="btn btn--primary"
              data-testid="call-button"
              onClick={handleCall}
            >
              <span className="btn__glyph">→</span>
              Call method
            </button>
          )}
        </div>
      </div>

      {/* Response */}
      {(response || error || streamLog.length > 0) && (
        <div className="console">
          <div className="console__head">
            <span className="console__title">
              {error
                ? "Error"
                : binding?.isStream
                  ? "Stream output"
                  : "Response"}
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
          {response && (
            <div className="console__body" data-testid="response-content">
              {response}
            </div>
          )}
          {streamLog.length > 0 && (
            <div className="console__body" data-testid="stream-log">
              {streamLog.map((entry, i) => {
                const isMeta = entry.startsWith("---");
                return (
                  <div
                    key={i}
                    className={`console__entry${isMeta ? " console__entry--meta" : ""}`}
                    data-testid="stream-entry"
                  >
                    <span className="console__entry-i">
                      {isMeta ? "··" : String(i + 1).padStart(2, "0")}
                    </span>
                    <span className="console__entry-body">{entry}</span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
