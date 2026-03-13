import { useParams, useNavigate } from 'react-router-dom';
import { getTypeById, methods, dataTypes } from '../data/types';
import { TypeString } from '../components/TypeLink';
import { ArrowLeft, ArrowRight } from 'lucide-react';

export default function TypeDetailPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const dt = getTypeById(id || '');

  if (!dt) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-slate-400">Type not found</p>
      </div>
    );
  }

  // Find methods that reference this type
  const referencingMethods = methods.filter(m => {
    const searchIn = [m.request, m.response, m.requestDescription || '', m.responseDescription || '', m.errorType || ''].join(' ');
    return searchIn.includes(dt.name) || searchIn.includes(dt.id);
  });

  // Find related types (types that reference this type, or that this type references)
  const relatedTypes = dataTypes.filter(t => {
    if (t.id === dt.id) return false;
    return t.definition.includes(dt.id) || t.definition.includes(dt.name) ||
      dt.definition.includes(t.id) || dt.definition.includes(t.name);
  });

  return (
    <div className="max-w-4xl mx-auto">
      {/* Breadcrumb */}
      <div className="flex items-center gap-2 text-sm text-slate-400 mb-6">
        <button onClick={() => navigate('/')} className="hover:text-white transition-colors">
          Host API
        </button>
        <span>/</span>
        <button onClick={() => navigate('/types')} className="hover:text-white transition-colors">
          Data Types
        </button>
        <span>/</span>
        <span className="text-white">{dt.name}</span>
      </div>

      {/* Header */}
      <div className="mb-8">
        <div className="flex items-center gap-3 mb-2">
          <h1 className="text-2xl font-bold text-white font-mono">{dt.name}</h1>
          <span className="text-xs px-2 py-0.5 rounded-full bg-slate-700/50 text-slate-400 border border-slate-600/30">
            {dt.category}
          </span>
        </div>
        <p className="text-slate-300 leading-relaxed">{dt.description}</p>
        {dt.source && (
          <p className="text-xs text-slate-500 mt-2 font-mono">
            Source: {dt.source}
          </p>
        )}
      </div>

      {/* Definition */}
      <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8">
        <div className="border-b border-slate-700/40 px-5 py-3">
          <h2 className="text-sm font-semibold text-white">Definition</h2>
        </div>
        <div className="px-5 py-4">
          <div className="bg-slate-900/60 rounded-lg p-4 font-mono text-sm">
            <TypeString text={dt.definition} />
          </div>
        </div>
      </div>

      {/* Fields */}
      {dt.fields && dt.fields.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white">Fields</h2>
          </div>
          <div className="divide-y divide-slate-700/30">
            {dt.fields.map((field, i) => (
              <div key={i} className="px-5 py-3 flex items-start gap-4">
                <code className="text-sm font-mono text-emerald-400 w-40 shrink-0">{field.name}</code>
                <div className="flex-1 min-w-0">
                  <TypeString text={field.type} />
                  <p className="text-xs text-slate-400 mt-0.5">{field.description}</p>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Variants */}
      {dt.variants && dt.variants.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white">Variants</h2>
          </div>
          <div className="divide-y divide-slate-700/30">
            {dt.variants.map((variant, i) => (
              <div key={i} className="px-5 py-3 flex items-start gap-4">
                <code className="text-sm font-mono text-amber-400 w-40 shrink-0">{variant.name}</code>
                <div className="flex-1 min-w-0">
                  <TypeString text={variant.type} />
                  <p className="text-xs text-slate-400 mt-0.5">{variant.description}</p>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Used by methods */}
      {referencingMethods.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white">Used by Methods</h2>
          </div>
          <div className="px-5 py-3 flex flex-wrap gap-2">
            {referencingMethods.map(m => (
              <button
                key={m.id}
                onClick={() => navigate(`/method/${m.id}`)}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-mono bg-slate-700/30 text-slate-300 hover:bg-slate-700/50 hover:text-white border border-slate-600/30 transition-colors"
              >
                <ArrowRight size={10} />
                {m.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Related types */}
      {relatedTypes.length > 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl overflow-hidden mb-8">
          <div className="border-b border-slate-700/40 px-5 py-3">
            <h2 className="text-sm font-semibold text-white">Related Types</h2>
          </div>
          <div className="px-5 py-3 flex flex-wrap gap-2">
            {relatedTypes.map(t => (
              <button
                key={t.id}
                onClick={() => navigate(`/type/${t.id}`)}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-mono bg-sky-500/10 text-sky-400 hover:bg-sky-500/20 border border-sky-500/20 transition-colors"
              >
                {t.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Back */}
      <button
        onClick={() => navigate('/types')}
        className="flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors mt-4"
      >
        <ArrowLeft size={16} />
        Back to all types
      </button>
    </div>
  );
}
