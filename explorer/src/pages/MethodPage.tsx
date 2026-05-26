import { Link, useOutletContext, useParams } from "react-router-dom";
import type { VersionEntry } from "../data/types";
import { findMethod, productFunction } from "../data/registry";
import PatternBadge from "../components/PatternBadge";
import CodeBlock from "../components/CodeBlock";
import { TypeString } from "../components/TypeLink";

/** Detail page for a single method. */
export default function MethodPage() {
  const { version } = useOutletContext<{ version: VersionEntry }>();
  const { methodName } = useParams<{ methodName: string }>();
  const found = methodName ? findMethod(version, methodName) : null;

  if (!found) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-slate-400">Method not found</p>
      </div>
    );
  }

  const { service, method } = found;
  const prefix = `/v/${version.id}`;

  return (
    <div className="max-w-4xl mx-auto">
      <div className="flex items-center gap-2 text-sm text-slate-400 mb-6 animate-fade-in">
        <Link to={`${prefix}/`} className="hover:text-white transition-colors">
          TrUAPI
        </Link>
        <span>/</span>
        <span className="text-slate-300">{service.name}</span>
        <span>/</span>
        <span className="text-white font-mono text-sm break-all">
          {method.name}
        </span>
      </div>

      <div className="mb-8 animate-slide-up">
        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2 sm:gap-4 mb-3">
          <h1 className="text-xl sm:text-2xl font-bold text-white font-mono break-all">
            {method.name}
          </h1>
          <PatternBadge kind={method.type} />
        </div>
        {method.description && (
          <p className="text-slate-300 leading-relaxed">{method.description}</p>
        )}
      </div>

      <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8 card-hover animate-slide-up">
        <div className="border-b border-slate-700/40 px-5 py-3">
          <h2 className="text-sm font-semibold text-white font-display">
            API Surface
          </h2>
        </div>
        <div className="divide-y divide-slate-700/30">
          <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
            <span className="text-sm text-slate-400 sm:w-36 shrink-0 pt-0.5">
              Product function
            </span>
            <code className="text-sm font-mono text-emerald-400 break-all">
              {productFunction(service, method)}
            </code>
          </div>
          {method.signature && (
            <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
              <span className="text-sm text-slate-400 sm:w-36 shrink-0 pt-0.5">
                Signature
              </span>
              <div className="min-w-0 flex-1">
                <CodeBlock code={method.signature} />
              </div>
            </div>
          )}
          {method.docUrl && (
            <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
              <span className="text-sm text-slate-400 sm:w-36 shrink-0 pt-0.5">
                Docs
              </span>
              <a
                href={method.docUrl}
                target="_blank"
                rel="noreferrer"
                className="text-sm font-mono text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors break-all"
              >
                {method.docUrl}
              </a>
            </div>
          )}
        </div>
      </div>

      {method.requestType && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-6 card-hover animate-slide-up">
          <div className="border-b border-slate-700/40 px-5 py-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-white font-display">
              {method.type === "subscription" ? "Start Payload" : "Request"}
            </h2>
            <TypeString
              text={method.requestType}
              versionId={version.id}
              types={version.types}
            />
          </div>
          {method.requestDescription && (
            <div className="px-5 py-3 text-sm text-slate-300">
              {method.requestDescription}
            </div>
          )}
        </div>
      )}

      {method.responseType && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-6 card-hover animate-slide-up">
          <div className="border-b border-slate-700/40 px-5 py-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-white font-display">
              {method.type === "subscription" ? "Receive Payload" : "Response"}
            </h2>
            <TypeString
              text={method.responseType}
              versionId={version.id}
              types={version.types}
            />
          </div>
        </div>
      )}

      {method.errorType && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-6 card-hover animate-slide-up">
          <div className="border-b border-slate-700/40 px-5 py-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-white font-display">
              Error
            </h2>
            <TypeString
              text={method.errorType}
              versionId={version.id}
              types={version.types}
            />
          </div>
        </div>
      )}

      {method.exampleSource && (
        <div className="mb-8 animate-slide-up">
          <h2 className="text-lg font-semibold text-white font-display mb-3">
            Example
          </h2>
          <CodeBlock code={method.exampleSource} />
        </div>
      )}
    </div>
  );
}
