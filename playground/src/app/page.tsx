"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  subscribeConnectionStatus,
  type ConnectionStatus,
} from "@/src/lib/transport";
import { ServiceTable } from "@/src/components/ServiceTable";
import { MethodView } from "@/src/components/MethodView";
import { AutoTestView } from "@/src/components/AutoTestView";
import { CommandPalette } from "@/src/components/CommandPalette";
import { services } from "@/src/lib/services";
import {
  type TestEntry,
  AUTO_TEST_ID,
  EXCLUDED_METHODS,
  runAutoTests,
  runSingleTest,
} from "@/src/lib/auto-test";

const STATUS_LABEL: Record<string, string> = {
  connected: "Host Linked",
  connecting: "Handshaking",
  disconnected: "Offline",
};

function StatusChip({ status }: { status: ConnectionStatus | null }) {
  const key = status ?? "connecting";
  const label = STATUS_LABEL[key] ?? key;
  return (
    <span className={`status status--${key}`} title={label}>
      <span className="status__led" aria-hidden />
      <span className="status__label">{label}</span>
    </span>
  );
}

function SearchTrigger({ onOpen }: { onOpen: () => void }) {
  const [isMac, setIsMac] = useState(false);
  useEffect(() => {
    setIsMac(/Mac|iPhone|iPad/.test(navigator.userAgent));
  }, []);
  return (
    <button
      type="button"
      className="search-btn"
      onClick={onOpen}
      aria-label="Search methods"
    >
      <span aria-hidden>⌕</span>
      <span className="search-btn__label">Search</span>
      <span className="search-btn__kbd">{isMac ? "⌘" : "Ctrl"}K</span>
    </button>
  );
}

function Masthead({
  status,
  onSearch,
}: {
  status: ConnectionStatus | null;
  onSearch: () => void;
}) {
  return (
    <header className="masthead">
      <div className="masthead__inner">
        <div className="wordmark">
          <span className="wordmark__dot" aria-hidden />
          <span className="wordmark__name">TrUAPI Playground</span>
          <span className="wordmark__tag">v0.2</span>
        </div>
        <div className="masthead__right">
          <SearchTrigger onOpen={onSearch} />
          <StatusChip status={status} />
        </div>
      </div>
    </header>
  );
}

function Splash({ status }: { status: ConnectionStatus | null }) {
  const connecting = status === null || status === "connecting";
  return (
    <div className="splash">
      <div className="splash__card">
        <div className="splash__eyebrow">
          <span className="wordmark__dot" aria-hidden />
          <span>TrUAPI Playground · v0.2</span>
        </div>
        <h1 className="splash__title">
          {connecting ? "Linking to host…" : "Host is offline."}
        </h1>
        <p className="splash__body">
          {connecting
            ? "Completing the postMessage handshake with the Polkadot Desktop Host. One moment."
            : "This playground must be opened from inside the Polkadot Desktop Host. While developing locally, launch it through:"}
        </p>
        {!connecting && (
          <code className="splash__code">https://dot.li/localhost:3000</code>
        )}
      </div>
    </div>
  );
}

export default function PlaygroundPage() {
  const [status, setStatus] = useState<ConnectionStatus | null>(null);
  const [selection, setSelection] = useState<{
    service: string;
    method: string;
  } | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [testResults, setTestResults] = useState<Record<string, TestEntry>>({});
  const [isTestRunning, setIsTestRunning] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    try {
      return subscribeConnectionStatus(setStatus);
    } catch {
      setStatus("disconnected");
    }
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      } else if (e.key === "Escape" && paletteOpen) {
        setPaletteOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [paletteOpen]);

  const handlePaletteSelect = useCallback((service: string, method: string) => {
    setSelection({ service, method });
    setPaletteOpen(false);
  }, []);

  const handleRunTests = useCallback(
    async (mode: "all" | "safe") => {
      if (isTestRunning) return;
      const excludeSet = mode === "safe" ? EXCLUDED_METHODS : new Set<string>();
      const controller = new AbortController();
      abortRef.current = controller;
      setIsTestRunning(true);
      const initial: Record<string, TestEntry> = {};
      for (const svc of services) {
        for (const m of svc.methods) {
          const id = `${svc.name}/${m.name}`;
          initial[id] = { status: excludeSet.has(id) ? "skipped" : "idle" };
        }
      }
      setTestResults(initial);
      try {
        await runAutoTests(
          services,
          (id, entry) => {
            setTestResults((prev) => ({ ...prev, [id]: entry }));
          },
          controller.signal,
          excludeSet,
        );
      } finally {
        setTestResults((prev) => {
          const updated = { ...prev };
          for (const [id, entry] of Object.entries(updated)) {
            if (entry.status === "running") updated[id] = { status: "idle" };
          }
          return updated;
        });
        setIsTestRunning(false);
      }
    },
    [isTestRunning],
  );

  const handleStopTests = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const handleRetryTest = useCallback(
    async (
      serviceName: string,
      methodName: string,
      requestOverride?: string,
    ) => {
      if (isTestRunning) return;
      await runSingleTest(
        services,
        serviceName,
        methodName,
        (id, entry) => {
          setTestResults((prev) => ({ ...prev, [id]: entry }));
        },
        requestOverride,
      );
    },
    [isTestRunning],
  );

  if (status === null || status === "connecting") {
    return <Splash status={status} />;
  }

  const hasView = selection !== null;
  const isAutoTest = selection?.service === AUTO_TEST_ID;

  return (
    <div className="shell">
      <Masthead status={status} onSearch={() => setPaletteOpen(true)} />
      <div className="board" data-has-view={hasView}>
        <aside className="rail">
          <p className="rail__intro">
            An interactive explorer for the TrUAPI surface exposed to products
            inside the Polkadot Desktop Browser.
          </p>
          <ServiceTable
            services={services}
            activeMethod={selection}
            testResults={testResults}
            onSelect={(s, m) => setSelection({ service: s, method: m })}
          />
        </aside>
        <section className="view">
          {isAutoTest ? (
            <AutoTestView
              services={services}
              testResults={testResults}
              isRunning={isTestRunning}
              onRun={handleRunTests}
              onStop={handleStopTests}
              onRetry={handleRetryTest}
              onBack={() => setSelection(null)}
            />
          ) : selection ? (
            <MethodView
              service={selection.service}
              method={selection.method}
              onBack={() => setSelection(null)}
            />
          ) : (
            <div className="empty-state">
              <span className="empty-state__mark">Ready</span>
              Pick a method from the index, or press <kbd>⌘K</kbd> to search.
            </div>
          )}
        </section>
      </div>
      {paletteOpen && (
        <CommandPalette
          services={services}
          onSelect={handlePaletteSelect}
          onClose={() => setPaletteOpen(false)}
        />
      )}
    </div>
  );
}
