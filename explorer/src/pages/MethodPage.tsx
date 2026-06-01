import { Link, useOutletContext, useParams } from "react-router-dom";
import type { VersionEntry } from "../data/types";
import { findMethod, productFunction } from "../data/registry";
import PatternBadge from "../components/PatternBadge";
import { TypeString } from "../components/TypeLink";
import { MarkdownText } from "../components/MarkdownText";
import CodeBlock from "../components/CodeBlock";

/** Deployed playground served inside the Polkadot Desktop Host. */
const HOSTED_PLAYGROUND_URL = "https://truapi-playground.dot.li";

/** Deep link that opens this method in the host-backed playground. */
function playgroundUrl(service: string, method: string): string {
  const params = new URLSearchParams({ service, method });
  return `${HOSTED_PLAYGROUND_URL}/?${params.toString()}`;
}

/** Detail page for a single method. */
export default function MethodPage() {
  const { version } = useOutletContext<{ version: VersionEntry }>();
  const { serviceName, methodName } = useParams<{
    serviceName: string;
    methodName: string;
  }>();
  const found =
    serviceName && methodName
      ? findMethod(version, serviceName, methodName)
      : null;

  if (!found) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-slate-400">Method not found</p>
      </div>
    );
  }

  const { service, method } = found;
  const prefix = `/v/${version.id}`;
  // Fall back to the raw id when the type isn't in the active version's list
  // (codegen drift defense). An unlinked id is still readable; an empty
  // string would produce `Result(, Err)` in the response row.
  const typeName = (id: string | undefined) =>
    id ? (version.types.find((t) => t.id === id)?.name ?? id) : "";
  const requestTypeName = typeName(method.requestType);
  const requestDescription =
    method.requestDescription && method.requestDescription !== requestTypeName
      ? method.requestDescription
      : null;
  const responseName = typeName(method.responseType);
  const errorName = typeName(method.errorType);
  const responseShape = method.responseType
    ? method.type === "subscription"
      ? `Stream<${responseName}>`
      : errorName
        ? `Result(${responseName}, ${errorName})`
        : responseName
    : null;

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
          <MarkdownText
            text={method.description}
            versionId={version.id}
            types={version.types}
            className="text-slate-300"
          />
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
          <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
            <span className="text-sm text-slate-400 sm:w-36 shrink-0 pt-0.5">
              Request
            </span>
            <div className="min-w-0 flex-1">
              {requestTypeName ? (
                <TypeString
                  text={requestTypeName}
                  versionId={version.id}
                  types={version.types}
                />
              ) : (
                <code className="text-sm font-mono text-slate-400">void</code>
              )}
              {requestDescription && (
                <MarkdownText
                  text={requestDescription}
                  versionId={version.id}
                  types={version.types}
                  className="text-sm text-slate-400 mt-1"
                />
              )}
            </div>
          </div>
          {responseShape && (
            <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
              <span className="text-sm text-slate-400 sm:w-36 shrink-0 pt-0.5">
                Response
              </span>
              <TypeString
                text={responseShape}
                versionId={version.id}
                types={version.types}
              />
            </div>
          )}
        </div>
      </div>

      {method.exampleSource && (
        <div className="mb-8 animate-slide-up">
          <div className="flex items-center justify-between gap-3 mb-3">
            <h2 className="text-sm font-semibold text-white font-display">
              Example
            </h2>
            <a
              href={playgroundUrl(service.name, method.name)}
              target="_blank"
              rel="noreferrer"
              className="text-xs font-medium text-pink-400 hover:text-pink-300 transition-colors whitespace-nowrap"
            >
              Run in playground ↗
            </a>
          </div>
          <CodeBlock code={method.exampleSource} />
          <p className="text-xs text-slate-500 mt-2">
            Open this example in the{" "}
            <a
              href={playgroundUrl(service.name, method.name)}
              target="_blank"
              rel="noreferrer"
              className="text-pink-400 hover:text-pink-300 transition-colors"
            >
              host-backed playground
            </a>{" "}
            to run it live.
          </p>
        </div>
      )}
    </div>
  );
}
