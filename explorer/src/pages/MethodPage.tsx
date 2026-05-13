import { AlertTriangle, ChevronLeft, ChevronRight } from "lucide-react";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import CodeBlock from "../components/CodeBlock";
import PatternBadge from "../components/PatternBadge";
import { TypeString } from "../components/TypeLink";
import { useVersion } from "../contexts/VersionContext";
import { versionedMethodRoutePath } from "../lib/routes";

export default function MethodPage() {
  const { groupId, id } = useParams<{ groupId: string; id: string }>();
  const navigate = useNavigate();
  const { getMethodById, getGroupById, methods, versionPrefix } = useVersion();
  const method = getMethodById(id || "");

  if (!method) {
    return (
      <div className="flex h-64 items-center justify-center">
        <p className="text-slate-400">Method not found</p>
      </div>
    );
  }

  if (groupId && groupId !== method.groupId) {
    return (
      <Navigate
        to={versionedMethodRoutePath(versionPrefix, method)}
        replace
      />
    );
  }

  const group = getGroupById(method.groupId);
  const groupMethods = methods.filter(
    (item) => item.groupId === method.groupId,
  );
  const currentIdx = groupMethods.findIndex((item) => item.id === method.id);
  const prevMethod = currentIdx > 0 ? groupMethods[currentIdx - 1] : null;
  const nextMethod =
    currentIdx < groupMethods.length - 1 ? groupMethods[currentIdx + 1] : null;
  const globalIdx = methods.findIndex((item) => item.id === method.id);
  const globalPrev = globalIdx > 0 ? methods[globalIdx - 1] : null;
  const globalNext =
    globalIdx < methods.length - 1 ? methods[globalIdx + 1] : null;
  const previous = prevMethod || globalPrev;
  const next = nextMethod || globalNext;
  const isSubscription =
    method.pattern === "subscription" ||
    method.pattern === "reverse-subscription";
  const responseType = method.errorType
    ? `Result<${method.response}, ${method.errorType}>`
    : method.response;

  return (
    <div className="mx-auto max-w-4xl">
      <div className="mb-6 flex items-center gap-2 text-sm text-slate-400 animate-fade-in">
        <button
          onClick={() => navigate(`${versionPrefix}/`)}
          className="transition-colors hover:text-white"
          type="button"
        >
          TrUAPI
        </button>
        <span>/</span>
        {groupMethods[0] ? (
          <button
            onClick={() => navigate(versionedMethodRoutePath(versionPrefix, groupMethods[0]))}
            className="transition-colors hover:text-white"
            type="button"
          >
            {group?.name}
          </button>
        ) : (
          <span>{group?.name}</span>
        )}
        <span>/</span>
        <span className="font-mono text-sm text-white">{method.name}</span>
      </div>

      <div className="mb-8 animate-slide-up">
        <div className="mb-3 flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
          <h1 className="break-all font-mono text-xl font-bold text-white sm:text-2xl">
            {method.name}
          </h1>
          <PatternBadge pattern={method.pattern} />
        </div>
        <p className="leading-relaxed text-slate-300">{method.description}</p>
        {method.notes && (
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-amber-500/20 bg-amber-500/5 p-3">
            <AlertTriangle
              size={16}
              className="mt-0.5 shrink-0 text-amber-400"
            />
            <p className="text-sm text-amber-200/80">{method.notes}</p>
          </div>
        )}
      </div>

      <div className="mb-8 overflow-hidden rounded-xl border border-slate-700/40 bg-slate-800/30 card-hover animate-slide-up stagger-1">
        <div className="border-b border-slate-700/40 px-5 py-3">
          <h2 className="font-display text-sm font-semibold text-white">
            API Surface
          </h2>
        </div>
        <div className="divide-y divide-slate-700/30">
          <div className="flex flex-col gap-1 px-5 py-3 sm:flex-row sm:items-start sm:gap-4">
            <span className="shrink-0 pt-0.5 text-sm text-slate-400 sm:w-32">
              {isSubscription ? "Start payload" : "Request"}
            </span>
            <div>
              <TypeString text={method.request} />
            </div>
          </div>
          <div className="flex flex-col gap-1 px-5 py-3 sm:flex-row sm:items-start sm:gap-4">
            <span className="shrink-0 pt-0.5 text-sm text-slate-400 sm:w-32">
              {isSubscription ? "Receive payload" : "Response"}
            </span>
            <div>
              <TypeString text={responseType} />
              {method.responseDescription && (
                <p className="mt-1 text-sm text-slate-400">
                  {method.responseDescription}
                </p>
              )}
            </div>
          </div>
        </div>
      </div>

      {method.usageExample && (
        <div className="mb-8 space-y-4 animate-slide-up stagger-2">
          <h2 className="font-display text-lg font-semibold text-white">
            Usage Example
          </h2>
          <CodeBlock code={method.usageExample} title="@parity/truapi" />
        </div>
      )}

      <div className="flex items-center justify-between border-t border-slate-700/40 pt-6 animate-fade-in stagger-4">
        {previous ? (
          <button
            onClick={() => navigate(versionedMethodRoutePath(versionPrefix, previous))}
            className="group flex items-center gap-2 text-sm text-slate-400 transition-colors hover:text-white"
            type="button"
          >
            <ChevronLeft
              size={16}
              className="transition-transform group-hover:-translate-x-0.5"
            />
            <div className="text-left">
              <div className="text-xs text-slate-400">Previous</div>
              <div className="font-mono text-xs">{previous.name}</div>
            </div>
          </button>
        ) : (
          <div />
        )}
        {next ? (
          <button
            onClick={() => navigate(versionedMethodRoutePath(versionPrefix, next))}
            className="group flex items-center gap-2 text-sm text-slate-400 transition-colors hover:text-white"
            type="button"
          >
            <div className="text-right">
              <div className="text-xs text-slate-400">Next</div>
              <div className="font-mono text-xs">{next.name}</div>
            </div>
            <ChevronRight
              size={16}
              className="transition-transform group-hover:translate-x-0.5"
            />
          </button>
        ) : (
          <div />
        )}
      </div>
    </div>
  );
}
