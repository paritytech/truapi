import { Link, useOutletContext } from "react-router-dom";
import type { VersionEntry } from "../data/types";
import { methodsByKind, productFunction, totalMethods } from "../data/registry";
import PatternBadge from "../components/PatternBadge";
import { MarkdownText } from "../components/MarkdownText";

/** Landing page: per-version stats and service cards. */
export default function OverviewPage() {
  const { version } = useOutletContext<{ version: VersionEntry }>();
  const total = totalMethods(version);
  const unary = methodsByKind(version, "unary").length;
  const subs = methodsByKind(version, "subscription").length;

  return (
    <div className="max-w-5xl mx-auto">
      <div className="mb-10 lg:mb-14 animate-slide-up">
        <div className="flex items-start gap-4 lg:gap-5 mb-6">
          <div className="w-12 h-12 lg:w-16 lg:h-16 rounded-xl lg:rounded-2xl bg-pink-600 flex items-center justify-center shrink-0 shadow-[0_0_40px_rgba(219,39,119,0.2)]">
            <span className="text-white text-lg lg:text-2xl font-bold font-display">
              T
            </span>
          </div>
          <div>
            <h1 className="text-2xl lg:text-4xl font-bold text-white font-display tracking-tight leading-tight">
              TrUAPI Service Surface
            </h1>
            <div className="flex flex-wrap items-center gap-2 lg:gap-3 mt-2">
              <span className="text-sm text-slate-400">
                Version{" "}
                <span className="font-mono text-slate-300">{version.id}</span>
              </span>
            </div>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-3 gap-3 lg:gap-4 mb-10 lg:mb-14">
        <div className="bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover">
          <div className="text-3xl font-bold text-white font-display">
            {total}
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Total Methods
          </div>
        </div>
        <div className="bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover">
          <div className="text-3xl font-bold text-white font-display">
            {unary}
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Request / Response
          </div>
        </div>
        <div className="bg-slate-800/40 border border-slate-700/40 rounded-xl p-5 card-hover">
          <div className="text-3xl font-bold text-white font-display">
            {subs}
          </div>
          <div className="text-sm text-slate-400 mt-1.5 font-medium">
            Subscriptions
          </div>
        </div>
      </div>

      <div className="animate-slide-up">
        <h2 className="text-xl font-semibold text-white mb-5 font-display tracking-tight">
          Services
        </h2>
        <div className="grid grid-cols-1 gap-3 mb-12">
          {version.services.map((service) => (
            <div
              key={service.name}
              className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 card-hover"
            >
              <div className="flex items-start justify-between gap-3 mb-3">
                <h3 className="font-semibold text-white text-sm font-display">
                  {service.name}
                </h3>
                <span className="text-xs text-slate-400">
                  {service.methods.length} methods
                </span>
              </div>
              <div className="divide-y divide-slate-700/30">
                {service.methods.map((method) => (
                  <Link
                    key={method.name}
                    to={`/v/${version.id}/method/${method.name}`}
                    className="flex items-start justify-between gap-3 py-2 group"
                  >
                    <div className="min-w-0">
                      <div className="font-mono text-sm text-slate-200 group-hover:text-white transition-colors break-all">
                        {productFunction(service, method)}
                      </div>
                      {method.description && (
                        <MarkdownText
                          text={method.description}
                          versionId={version.id}
                          types={version.types}
                          className="text-xs text-slate-400 mt-0.5"
                          hideCodeBlocks
                        />
                      )}
                    </div>
                    <PatternBadge kind={method.type} />
                  </Link>
                ))}
              </div>
            </div>
          ))}
          {version.services.length === 0 && (
            <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 text-sm text-slate-400">
              No services available in this version.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
