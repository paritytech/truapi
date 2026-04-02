import { useParams, useNavigate } from 'react-router-dom';
import { useVersion } from '../contexts/VersionContext';
import PatternBadge from '../components/PatternBadge';
import CodeBlock from '../components/CodeBlock';
import { TypeString } from '../components/TypeLink';
import { ChevronLeft, ChevronRight, AlertTriangle } from 'lucide-react';

export default function MethodPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { getMethodById, getGroupById, methods, versionPrefix } = useVersion();
  const method = getMethodById(id || '');

  if (!method) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-slate-400">Method not found</p>
      </div>
    );
  }

  const group = getGroupById(method.groupId);
  const groupMethods = methods.filter(m => m.groupId === method.groupId);
  const currentIdx = groupMethods.findIndex(m => m.id === method.id);
  const prevMethod = currentIdx > 0 ? groupMethods[currentIdx - 1] : null;
  const nextMethod = currentIdx < groupMethods.length - 1 ? groupMethods[currentIdx + 1] : null;

  // Also support global prev/next
  const globalIdx = methods.findIndex(m => m.id === method.id);
  const globalPrev = globalIdx > 0 ? methods[globalIdx - 1] : null;
  const globalNext = globalIdx < methods.length - 1 ? methods[globalIdx + 1] : null;

  return (
    <div className="max-w-4xl mx-auto">
      {/* Breadcrumb */}
      <div className="flex items-center gap-2 text-sm text-slate-400 mb-6 animate-fade-in">
        <button onClick={() => navigate(versionPrefix + '/')} className="hover:text-white transition-colors">
          TrUAPI
        </button>
        <span>/</span>
        <button
          onClick={() => navigate(`${versionPrefix}/method/${groupMethods[0]?.id}`)}
          className="hover:text-white transition-colors"
        >
          {group?.name}
        </button>
        <span>/</span>
        <span className="text-white font-mono text-sm">{method.name}</span>
      </div>

      {/* Header */}
      <div className="mb-8 animate-slide-up">
        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2 sm:gap-4 mb-3">
          <h1 className="text-xl sm:text-2xl font-bold text-white font-mono break-all">{method.name}</h1>
          <PatternBadge pattern={method.pattern} />
        </div>
        <p className="text-slate-300 leading-relaxed">{method.description}</p>
        {method.notes && (
          <div className="mt-3 flex items-start gap-2 bg-amber-500/5 border border-amber-500/20 rounded-lg p-3">
            <AlertTriangle size={16} className="text-amber-400 shrink-0 mt-0.5" />
            <p className="text-sm text-amber-200/80">{method.notes}</p>
          </div>
        )}
      </div>

      {/* API Surface */}
      <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8 card-hover animate-slide-up stagger-1">
        <div className="border-b border-slate-700/40 px-5 py-3">
          <h2 className="text-sm font-semibold text-white font-display">API Surface</h2>
        </div>
        <div className="divide-y divide-slate-700/30">
          <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
            <span className="text-sm text-slate-400 sm:w-32 shrink-0 pt-0.5">Product function</span>
            <code className="text-sm font-mono text-emerald-400">{method.productFunction}</code>
          </div>
          <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
            <span className="text-sm text-slate-400 sm:w-32 shrink-0 pt-0.5">Host handler</span>
            <code className="text-sm font-mono text-purple-400">{method.hostHandler}</code>
          </div>
          <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
            <span className="text-sm text-slate-400 sm:w-32 shrink-0 pt-0.5">
              {method.pattern === 'subscription' || method.pattern === 'reverse-subscription' ? 'Start payload' : 'Request'}
            </span>
            <div>
              <TypeString text={method.request} />
              {method.requestDescription && (
                <p className="text-sm text-slate-400 mt-1">{method.requestDescription}</p>
              )}
            </div>
          </div>
          <div className="px-5 py-3 flex flex-col sm:flex-row sm:items-start gap-1 sm:gap-4">
            <span className="text-sm text-slate-400 sm:w-32 shrink-0 pt-0.5">
              {method.pattern === 'subscription' || method.pattern === 'reverse-subscription' ? 'Receive payload' : 'Response'}
            </span>
            <div>
              <TypeString text={method.response} />
              {method.responseDescription && (
                <p className="text-sm text-slate-400 mt-1">{method.responseDescription}</p>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Error types */}
      {method.errorType && method.errorVariants && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8 card-hover animate-slide-up stagger-2">
          <div className="border-b border-slate-700/40 px-5 py-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-white font-display">Error Type</h2>
            <TypeString text={method.errorType} />
          </div>
          <div className="px-5 py-3">
            <div className="flex flex-wrap gap-2">
              {method.errorVariants.map((variant, i) => (
                <span
                  key={i}
                  className="inline-flex items-center px-2.5 py-1 rounded-md text-xs font-mono bg-red-500/10 text-red-300 border border-red-500/20"
                >
                  {variant}
                </span>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Code Examples */}
      <div className="space-y-4 mb-8 animate-slide-up stagger-3">
        <h2 className="text-lg font-semibold text-white font-display">Usage Examples</h2>
        <CodeBlock code={method.productExample} title="Product Side" />
        <CodeBlock code={method.hostExample} title="Host Side" />
      </div>

      {/* Navigation */}
      <div className="flex items-center justify-between pt-6 border-t border-slate-700/40 animate-fade-in stagger-4">
        {(prevMethod || globalPrev) ? (
          <button
            onClick={() => navigate(`${versionPrefix}/method/${(prevMethod || globalPrev)!.id}`)}
            className="flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors group"
          >
            <ChevronLeft size={16} className="group-hover:-translate-x-0.5 transition-transform" />
            <div className="text-left">
              <div className="text-xs text-slate-400">Previous</div>
              <div className="font-mono text-xs">{(prevMethod || globalPrev)!.name}</div>
            </div>
          </button>
        ) : <div />}
        {(nextMethod || globalNext) ? (
          <button
            onClick={() => navigate(`${versionPrefix}/method/${(nextMethod || globalNext)!.id}`)}
            className="flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors group"
          >
            <div className="text-right">
              <div className="text-xs text-slate-400">Next</div>
              <div className="font-mono text-xs">{(nextMethod || globalNext)!.name}</div>
            </div>
            <ChevronRight size={16} className="group-hover:translate-x-0.5 transition-transform" />
          </button>
        ) : <div />}
      </div>
    </div>
  );
}
