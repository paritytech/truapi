import { Routes, Route } from 'react-router-dom';
import Sidebar from './components/Sidebar';
import OverviewPage from './pages/OverviewPage';
import MethodPage from './pages/MethodPage';
import TypesPage from './pages/TypesPage';
import TypeDetailPage from './pages/TypeDetailPage';
import { Search } from 'lucide-react';
import { useState, useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { methods, dataTypes } from './data/types';

function SearchModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();

  useEffect(() => {
    if (open) {
      setQuery('');
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  if (!open) return null;

  const results: { type: 'method' | 'type'; id: string; name: string; description: string }[] = [];

  if (query.length > 0) {
    const q = query.toLowerCase();
    for (const m of methods) {
      if (m.name.toLowerCase().includes(q) || m.description.toLowerCase().includes(q)) {
        results.push({ type: 'method', id: m.id, name: m.name, description: m.description });
      }
    }
    for (const t of dataTypes) {
      if (t.name.toLowerCase().includes(q) || t.description.toLowerCase().includes(q)) {
        results.push({ type: 'type', id: t.id, name: t.name, description: t.description });
      }
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]" onClick={onClose}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <div
        className="relative bg-slate-850 border border-slate-700/60 rounded-xl w-full max-w-xl shadow-2xl overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700/40">
          <Search size={18} className="text-slate-400" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Search methods and types..."
            value={query}
            onChange={e => setQuery(e.target.value)}
            className="flex-1 bg-transparent text-white placeholder:text-slate-500 focus:outline-none text-sm"
            onKeyDown={e => {
              if (e.key === 'Escape') onClose();
              if (e.key === 'Enter' && results.length > 0) {
                const r = results[0];
                navigate(r.type === 'method' ? `/method/${r.id}` : `/type/${r.id}`);
                onClose();
              }
            }}
          />
          <kbd className="text-[10px] text-slate-500 bg-slate-800 border border-slate-700/50 px-1.5 py-0.5 rounded">ESC</kbd>
        </div>

        {results.length > 0 && (
          <div className="max-h-80 overflow-y-auto py-2">
            {results.slice(0, 20).map((r, i) => (
              <button
                key={`${r.type}-${r.id}-${i}`}
                onClick={() => {
                  navigate(r.type === 'method' ? `/method/${r.id}` : `/type/${r.id}`);
                  onClose();
                }}
                className="w-full text-left px-4 py-2 hover:bg-slate-800/60 transition-colors flex items-start gap-3"
              >
                <span className={`text-[10px] uppercase font-semibold px-1.5 py-0.5 rounded mt-0.5 ${
                  r.type === 'method' ? 'bg-emerald-500/10 text-emerald-400' : 'bg-sky-500/10 text-sky-400'
                }`}>
                  {r.type === 'method' ? 'FN' : 'T'}
                </span>
                <div className="min-w-0">
                  <div className="font-mono text-sm text-white truncate">{r.name}</div>
                  <div className="text-xs text-slate-400 truncate">{r.description}</div>
                </div>
              </button>
            ))}
          </div>
        )}

        {query.length > 0 && results.length === 0 && (
          <div className="py-8 text-center text-sm text-slate-500">No results found</div>
        )}

        {query.length === 0 && (
          <div className="py-6 text-center text-sm text-slate-500">
            Start typing to search {methods.length} methods and {dataTypes.length} types
          </div>
        )}
      </div>
    </div>
  );
}

function App() {
  const [searchOpen, setSearchOpen] = useState(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setSearchOpen(prev => !prev);
      }
      if (e.key === 'Escape') {
        setSearchOpen(false);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  return (
    <div className="flex min-h-screen bg-slate-925">
      <Sidebar />

      <main className="flex-1 min-w-0">
        {/* Top bar */}
        <div className="sticky top-0 z-10 bg-slate-925/80 backdrop-blur-md border-b border-slate-700/40">
          <div className="flex items-center justify-between px-8 py-3">
            <button
              onClick={() => setSearchOpen(true)}
              className="flex items-center gap-2 bg-slate-800/50 border border-slate-700/40 rounded-lg px-3 py-1.5 text-sm text-slate-400 hover:text-slate-200 hover:border-slate-600/50 transition-colors"
            >
              <Search size={14} />
              <span>Search...</span>
              <kbd className="text-[10px] text-slate-500 bg-slate-700/50 px-1.5 py-0.5 rounded ml-4">
                {navigator.platform.includes('Mac') ? '\u2318' : 'Ctrl'}K
              </kbd>
            </button>
          </div>
        </div>

        {/* Content */}
        <div className="px-8 py-8">
          <Routes>
            <Route path="/" element={<OverviewPage />} />
            <Route path="/method/:id" element={<MethodPage />} />
            <Route path="/types" element={<TypesPage />} />
            <Route path="/type/:id" element={<TypeDetailPage />} />
          </Routes>
        </div>
      </main>

      <SearchModal open={searchOpen} onClose={() => setSearchOpen(false)} />
    </div>
  );
}

export default App;
