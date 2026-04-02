import { useNavigate } from 'react-router-dom';
import { useVersion } from '../contexts/VersionContext';
import { Search } from 'lucide-react';
import { useState } from 'react';

export default function TypesPage() {
  const navigate = useNavigate();
  const { dataTypes, versionPrefix } = useVersion();
  const [search, setSearch] = useState('');

  const filtered = search
    ? dataTypes.filter(t =>
        t.name.toLowerCase().includes(search.toLowerCase()) ||
        t.description.toLowerCase().includes(search.toLowerCase()) ||
        t.category.toLowerCase().includes(search.toLowerCase())
      )
    : dataTypes;

  const categoryMap = new Map<string, typeof filtered>();
  for (const t of filtered) {
    if (!categoryMap.has(t.category)) categoryMap.set(t.category, []);
    categoryMap.get(t.category)!.push(t);
  }

  return (
    <div className="max-w-5xl mx-auto">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2 mb-8 animate-slide-up">
        <div>
          <h1 className="text-2xl font-bold text-white mb-1 font-display tracking-tight">Data Types</h1>
          <p className="text-sm text-slate-400">
            All types use SCALE codec primitives from <code className="text-xs font-mono bg-slate-800 px-1 py-0.5 rounded">scale-ts</code> and <code className="text-xs font-mono bg-slate-800 px-1 py-0.5 rounded">@novasamatech/scale</code>.
          </p>
        </div>
        <div className="text-sm text-slate-400 font-display">{dataTypes.length} types</div>
      </div>

      {/* Search */}
      <div className="relative mb-8 animate-slide-up stagger-1">
        <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
        <input
          type="text"
          placeholder="Search types..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full bg-slate-800/50 border border-slate-700/50 rounded-lg pl-10 pr-4 py-2.5 text-sm text-white placeholder:text-slate-500 focus:outline-none focus:border-pink-500/50 focus:ring-1 focus:ring-pink-500/20 transition-all duration-200"
        />
      </div>

      {/* Categories */}
      {Array.from(categoryMap.entries()).map(([category, types], catIdx) => (
        <div key={category} className="mb-8 animate-slide-up" style={{ animationDelay: `${0.1 + catIdx * 0.06}s` }}>
          <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wider mb-3 flex items-center gap-2 font-display">
            <span className="w-2 h-2 rounded-full bg-pink-500/60" />
            {category}
            <span className="text-xs font-normal text-slate-400 lowercase">({types.length})</span>
          </h2>
          <div className="grid grid-cols-1 gap-1.5">
            {types.map(t => (
              <button
                key={t.id}
                onClick={() => navigate(`${versionPrefix}/type/${t.id}`)}
                className="bg-slate-800/30 border border-slate-700/30 rounded-lg px-4 py-3 text-left hover:border-slate-600/50 hover:bg-slate-800/50 transition-all duration-150 group hover:shadow-[0_4px_12px_rgba(0,0,0,0.2)]"
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0">
                    <span className="font-mono text-sm text-sky-400 group-hover:text-sky-300 transition-colors">
                      {t.name}
                    </span>
                    <p className="text-sm text-slate-400 mt-0.5 truncate">{t.description}</p>
                  </div>
                  <code className="text-xs font-mono text-slate-400 shrink-0 bg-slate-800/50 px-2 py-0.5 rounded hidden sm:block">
                    {t.definition.length > 40 ? t.definition.slice(0, 40) + '...' : t.definition}
                  </code>
                </div>
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
