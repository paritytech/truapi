"use client";

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  subscribeConnectionStatus,
  type ConnectionStatus,
} from "@/src/lib/transport";
import { ServiceTable } from "@/src/components/ServiceTable";
import { MethodView } from "@/src/components/MethodView";
import { DiagnosisView } from "@/src/components/DiagnosisView";
import { CommandPalette } from "@/src/components/CommandPalette";
import { services } from "@/src/lib/services";
import {
  type TestEntry,
  DIAGNOSIS_ID,
  runDiagnosis,
  runSingleTest,
} from "@/src/lib/auto-test";
import packageJson from "../../package.json";

const VERSION_LABEL = `v${packageJson.version}`;

// Run the scroll-restore synchronously after the index re-mounts so the
// previously-open row is centered before paint (no flash of scroll-top).
const useIsoLayoutEffect =
  typeof window !== "undefined" ? useLayoutEffect : useEffect;

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
      <svg
        className="search-btn__icon"
        xmlns="http://www.w3.org/2000/svg"
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-3.5-3.5" />
      </svg>
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
          <span className="wordmark__tag">{VERSION_LABEL}</span>
        </div>
        <div className="masthead__right">
          {status !== "connected" && (
            <a
              className="open-in-dotli"
              href="https://truapi-playground.dot.li"
              target="_blank"
              rel="noreferrer"
              title="Open this playground inside the Polkadot Desktop Host"
            >
              Open in dotli ↗
            </a>
          )}
          <SearchTrigger onOpen={onSearch} />
          <StatusChip status={status} />
        </div>
      </div>
    </header>
  );
}

type Selection = { service: string; method: string } | null;

// The Diagnosis screen is deep-linked with a clean `?view=` param rather than
// its internal service id.
const VIEW_PARAM: Record<string, string> = {
  [DIAGNOSIS_ID]: "diagnosis",
};
const SERVICE_FOR_VIEW: Record<string, string> = {
  diagnosis: DIAGNOSIS_ID,
};

function selectionFromUrl(): Selection {
  if (typeof window === "undefined") return null;
  const params = new URLSearchParams(window.location.search);
  const view = params.get("view");
  if (view && SERVICE_FOR_VIEW[view]) {
    return { service: SERVICE_FOR_VIEW[view], method: "" };
  }
  const service = params.get("service");
  const method = params.get("method");
  if (!service) return null;
  return { service, method: method ?? "" };
}

function urlForSelection(selection: Selection): string {
  if (!selection) return window.location.pathname;
  const params = new URLSearchParams();
  const view = VIEW_PARAM[selection.service];
  if (view) {
    params.set("view", view);
  } else {
    params.set("service", selection.service);
    if (selection.method) params.set("method", selection.method);
  }
  return `${window.location.pathname}?${params.toString()}`;
}

export default function PlaygroundPage() {
  const [status, setStatus] = useState<ConnectionStatus | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  // The last method viewed, kept highlighted in the index after "← INDEX".
  const [lastViewed, setLastViewed] = useState<Selection>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [testResults, setTestResults] = useState<Record<string, TestEntry>>({});
  const [isTestRunning, setIsTestRunning] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  // The method open when "← INDEX" was clicked, so the index can re-center on
  // it instead of jumping to the top.
  const pendingScrollRef = useRef<Selection>(null);

  // Hydrate selection from the URL on mount and respond to back/forward.
  useEffect(() => {
    setSelection(selectionFromUrl());
    const onPop = () => setSelection(selectionFromUrl());
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  // Reflect selection changes into the URL via pushState (skip if already
  // matching, otherwise back/forward navigation loops).
  useEffect(() => {
    if (typeof window === "undefined") return;
    const next = urlForSelection(selection);
    if (next !== window.location.pathname + window.location.search) {
      window.history.pushState({}, "", next);
    }
  }, [selection]);

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

  // Going back to the index: remember the open method so the index can scroll
  // it into the center of view rather than resetting to the top.
  const handleBack = useCallback(() => {
    pendingScrollRef.current = selection;
    setLastViewed(selection);
    setSelection(null);
  }, [selection]);

  useIsoLayoutEffect(() => {
    if (selection !== null) return;
    const target = pendingScrollRef.current;
    pendingScrollRef.current = null;
    if (!target?.method) return;
    const el = document.querySelector(
      `[data-testid="method-${target.service}-${target.method}"]`,
    );
    if (el instanceof HTMLElement) {
      el.scrollIntoView({ block: "center" });
      el.focus({ preventScroll: true });
    }
  }, [selection]);

  const handleRunDiagnosis = useCallback(async () => {
    if (isTestRunning) return;
    const controller = new AbortController();
    abortRef.current = controller;
    setIsTestRunning(true);
    const initial: Record<string, TestEntry> = {};
    for (const svc of services) {
      for (const m of svc.methods) {
        initial[`${svc.name}/${m.name}`] = { status: "idle" };
      }
    }
    setTestResults(initial);
    try {
      await runDiagnosis(
        services,
        (id, entry) => {
          setTestResults((prev) => ({ ...prev, [id]: entry }));
        },
        controller.signal,
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
  }, [isTestRunning]);

  const handleStopTests = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const handleRetryDiagnosis = useCallback(
    async (serviceName: string, methodName: string) => {
      if (isTestRunning) return;
      await runSingleTest(services, serviceName, methodName, (id, entry) => {
        setTestResults((prev) => ({ ...prev, [id]: entry }));
      });
    },
    [isTestRunning],
  );

  const hasView = selection !== null;
  const isDiagnosis = selection?.service === DIAGNOSIS_ID;

  return (
    <div className="shell">
      <Masthead status={status} onSearch={() => setPaletteOpen(true)} />
      <div className="board" data-has-view={hasView}>
        <aside className="rail">
          <p className="rail__intro">
            An interactive playground for the TrUAPI surface exposed to products
            inside the Polkadot Desktop Browser.
          </p>
          <ServiceTable
            services={services}
            activeMethod={selection ?? lastViewed}
            testResults={testResults}
            onSelect={(s, m) => setSelection({ service: s, method: m })}
          />
        </aside>
        <section className="view">
          {isDiagnosis ? (
            <DiagnosisView
              services={services}
              testResults={testResults}
              isRunning={isTestRunning}
              onRun={handleRunDiagnosis}
              onStop={handleStopTests}
              onRetry={handleRetryDiagnosis}
              onBack={handleBack}
            />
          ) : selection ? (
            <MethodView
              service={selection.service}
              method={selection.method}
              onBack={handleBack}
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
