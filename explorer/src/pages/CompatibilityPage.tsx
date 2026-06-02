import { Link, useOutletContext } from "react-router-dom";
import { Check, Minus, X } from "lucide-react";
import type { VersionEntry } from "../data/types";
import { methodPath } from "../data/registry";
import { compatibility } from "../data/compatibility";
import type { CompatStatus } from "../data/compatibility-types";
import { playgroundDiagnosisUrl } from "../data/playground";

/** Per-method host compatibility, aggregated from per-host diagnosis reports. */
export default function CompatibilityPage() {
  const { version } = useOutletContext<{ version: VersionEntry }>();
  const { generatedAt, hosts, methods } = compatibility;

  if (hosts.length === 0) {
    return (
      <div className="max-w-4xl mx-auto">
        <h1 className="text-2xl lg:text-3xl font-bold text-white font-display tracking-tight mb-3">
          Host compatibility
        </h1>
        <p className="text-slate-400 mb-6">
          No host data yet. Run the{" "}
          <a
            href={playgroundDiagnosisUrl()}
            target="_blank"
            rel="noreferrer"
            className="font-medium text-pink-400 hover:text-pink-300 transition-colors"
          >
            Diagnosis in the playground
          </a>{" "}
          for each host you want covered, drop the reports into{" "}
          <code className="font-mono text-slate-200">
            explorer/pending-reports/
          </code>
          , and run{" "}
          <code className="font-mono text-slate-200">npm run generate-matrix</code>{" "}
          from the explorer.
        </p>
      </div>
    );
  }

  const byId = new Map(methods.map((m) => [m.id, m]));

  return (
    <div className="max-w-5xl mx-auto">
      <div className="mb-8 animate-slide-up">
        <div className="flex items-start justify-between gap-3">
          <h1 className="text-2xl lg:text-3xl font-bold text-white font-display tracking-tight">
            Host compatibility
          </h1>
          <a
            href={playgroundDiagnosisUrl()}
            target="_blank"
            rel="noreferrer"
            className="shrink-0 text-xs font-medium text-pink-400 hover:text-pink-300 transition-colors whitespace-nowrap mt-1"
          >
            Re-run diagnosis ↗
          </a>
        </div>
        <p className="text-sm text-slate-400 mt-2">
          Aggregated from {hosts.length} host{hosts.length === 1 ? "" : "s"} —
          generated{" "}
          <span className="font-mono text-slate-300">{generatedAt}</span>.
        </p>
        <div className="flex flex-wrap items-center gap-4 mt-4 text-xs text-slate-400">
          <span className="inline-flex items-center gap-1.5">
            <PassIcon />
            <span>pass</span>
          </span>
          <span className="inline-flex items-center gap-1.5">
            <FailIcon />
            <span>fail</span>
          </span>
          <span className="inline-flex items-center gap-1.5">
            <NotReportedIcon />
            <span>not reported</span>
          </span>
        </div>
      </div>

      <div className="overflow-auto max-h-[70vh] bg-slate-800/30 border border-slate-700/50 rounded-xl shadow-[0_4px_24px_rgba(0,0,0,0.25)]">
        <table className="w-full border-separate border-spacing-0">
          <thead>
            <tr>
              <th className="sticky left-0 top-0 z-30 text-left text-[11px] font-semibold uppercase tracking-wider text-slate-400 font-display px-5 py-3 bg-slate-900 border-b border-slate-700/60">
                Method
              </th>
              {hosts.map((h) => (
                <th
                  key={h.label}
                  className="sticky top-0 z-20 text-center text-[11px] font-semibold uppercase tracking-wider text-slate-400 font-display px-5 py-3 w-28 whitespace-nowrap bg-slate-900 border-b border-l border-slate-700/60"
                  title={`Reported ${h.reportedAt || "(unknown time)"}`}
                >
                  {h.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {version.services.map((service, i) => (
              <ServiceRows
                key={service.name}
                serviceName={service.name}
                serviceIndex={i}
                methods={service.methods.map((m) => ({
                  name: m.name,
                  results: byId.get(`${service.name}/${m.name}`)?.results,
                }))}
                hosts={hosts.map((h) => h.label)}
                versionId={version.id}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function ServiceRows({
  serviceName,
  serviceIndex,
  methods,
  hosts,
  versionId,
}: {
  serviceName: string;
  serviceIndex: number;
  methods: Array<{ name: string; results?: Record<string, CompatStatus | null> }>;
  hosts: string[];
  versionId: string;
}) {
  const colCount = hosts.length + 1;
  return (
    <>
      <tr>
        <td
          colSpan={colCount}
          className={`px-5 py-2 bg-slate-900/50 ${
            serviceIndex === 0 ? "" : "border-t border-slate-700/40"
          }`}
        >
          <div className="flex items-baseline gap-3">
            <span className="text-[11px] font-semibold uppercase tracking-[0.16em] text-pink-400 font-display">
              {serviceName}
            </span>
            <span className="text-[10px] text-slate-500">
              {methods.length} method{methods.length === 1 ? "" : "s"}
            </span>
          </div>
        </td>
      </tr>
      {methods.map((m, i) => {
        const last = i === methods.length - 1;
        return (
          <tr
            key={m.name}
            className="group hover:bg-slate-800/40 transition-colors"
          >
            <td
              className={`sticky left-0 z-[1] px-5 py-2 font-mono text-sm whitespace-nowrap bg-slate-925 group-hover:bg-slate-800/40 transition-colors ${
                last ? "" : "border-b border-slate-700/25"
              }`}
            >
              <Link
                to={methodPath(versionId, serviceName, m.name)}
                className="text-slate-200 hover:text-pink-300 transition-colors"
              >
                {m.name}
              </Link>
            </td>
            {hosts.map((label) => (
              <StatusCell
                key={label}
                status={m.results?.[label]}
                last={last}
              />
            ))}
          </tr>
        );
      })}
    </>
  );
}

function StatusCell({
  status,
  last,
}: {
  status: CompatStatus | null | undefined;
  last: boolean;
}) {
  const base = `px-5 py-2 text-center align-middle border-l border-slate-700/25 ${
    last ? "" : "border-b border-slate-700/25"
  }`;
  if (status === "pass") {
    return (
      <td className={base}>
        <PassIcon />
      </td>
    );
  }
  if (status === "fail") {
    return (
      <td className={base}>
        <FailIcon />
      </td>
    );
  }
  return (
    <td className={base} title="not reported">
      <NotReportedIcon />
    </td>
  );
}

function PassIcon() {
  return (
    <span
      className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-emerald-500/15 text-emerald-300 ring-1 ring-emerald-500/30"
      aria-label="pass"
    >
      <Check size={14} strokeWidth={2.5} />
    </span>
  );
}

function FailIcon() {
  return (
    <span
      className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-rose-500/15 text-rose-300 ring-1 ring-rose-500/30"
      aria-label="fail"
    >
      <X size={14} strokeWidth={2.5} />
    </span>
  );
}

function NotReportedIcon() {
  return (
    <span
      className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-slate-800/60 text-slate-500 ring-1 ring-slate-700/50"
      aria-label="not reported"
    >
      <Minus size={14} strokeWidth={2.5} />
    </span>
  );
}
