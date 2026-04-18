import { Routes, Route, Navigate, useLocation, useParams } from 'react-router-dom';
import Sidebar from './components/Sidebar';
import OverviewPage from './pages/OverviewPage';
import MethodPage from './pages/MethodPage';
import TypesPage from './pages/TypesPage';
import TypeDetailPage from './pages/TypeDetailPage';
import { Search, Menu } from 'lucide-react';
import { useState, useEffect, useRef, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { getVersion, defaultVersion } from './data/registry';
import { VersionProvider, useVersion } from './contexts/VersionContext';

function highlightMatch(text: string, query: string): React.ReactNode {
  if (!query) return text;
  const idx = text.toLowerCase().indexOf(query.toLowerCase());
  if (idx === -1) return text;
  return (
    <>
      {text.slice(0, idx)}
      <span className="text-white bg-pink-500/20 rounded px-0.5">{text.slice(idx, idx + query.length)}</span>
      {text.slice(idx + query.length)}
    </>
  );
}

function SearchModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();
  const { methods, dataTypes, versionPrefix } = useVersion();

  useEffect(() => {
    if (open) {
      setQuery('');
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const { methodResults, typeResults, allResults } = useMemo(() => {
    if (query.length === 0) return { methodResults: [], typeResults: [], allResults: [] };
    const q = query.toLowerCase();
    const mr: typeof methods = [];
    const tr: typeof dataTypes = [];
    for (const m of methods) {
      if (m.name.toLowerCase().includes(q) || m.description.toLowerCase().includes(q)) {
        mr.push(m);
      }
    }
    for (const t of dataTypes) {
      if (t.name.toLowerCase().includes(q) || t.description.toLowerCase().includes(q)) {
        tr.push(t);
      }
    }
    const all: { type: 'method' | 'type'; id: string; name: string; description: string }[] = [
      ...mr.map(m => ({ type: 'method' as const, id: m.id, name: m.name, description: m.description })),
      ...tr.map(t => ({ type: 'type' as const, id: t.id, name: t.name, description: t.description })),
    ];
    return { methodResults: mr, typeResults: tr, allResults: all };
  }, [query, methods, dataTypes]);

  const go = (r: { type: string; id: string }) => {
    navigate(r.type === 'method' ? `${versionPrefix}/method/${r.id}` : `${versionPrefix}/type/${r.id}`);
    onClose();
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]" onClick={onClose}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-fade-in" />
      <div
        className="relative bg-slate-850 border border-slate-700/60 rounded-xl w-full max-w-xl shadow-2xl overflow-hidden animate-scale-in"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700/40">
          <Search size={18} className="text-slate-400" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Search methods and types..."
            value={query}
            onChange={e => { setQuery(e.target.value); setSelectedIdx(0); }}
            className="flex-1 bg-transparent text-white placeholder:text-slate-500 focus:outline-none text-sm"
            onKeyDown={e => {
              if (e.key === 'Escape') onClose();
              if (e.key === 'ArrowDown') {
                e.preventDefault();
                setSelectedIdx(i => Math.min(i + 1, allResults.length - 1));
              }
              if (e.key === 'ArrowUp') {
                e.preventDefault();
                setSelectedIdx(i => Math.max(i - 1, 0));
              }
              if (e.key === 'Enter' && allResults.length > 0) {
                go(allResults[selectedIdx]);
              }
            }}
          />
          <kbd className="text-xs text-slate-400 bg-slate-800 border border-slate-700/50 px-1.5 py-0.5 rounded font-mono">ESC</kbd>
        </div>

        {query.length > 0 && (methodResults.length > 0 || typeResults.length > 0) && (
          <div className="max-h-80 overflow-y-auto py-1">
            {methodResults.length > 0 && (
              <>
                <div className="px-4 pt-2 pb-1 text-xs uppercase tracking-wider text-slate-400 font-semibold font-display">
                  Methods
                </div>
                {methodResults.slice(0, 10).map((m) => {
                  const globalIdx = allResults.findIndex(r => r.type === 'method' && r.id === m.id);
                  return (
                    <button
                      key={`method-${m.id}`}
                      onClick={() => go({ type: 'method', id: m.id })}
                      className={`w-full text-left px-4 py-2 transition-colors flex items-start gap-3 ${
                        globalIdx === selectedIdx ? 'bg-slate-800/80' : 'hover:bg-slate-800/60'
                      }`}
                    >
                      <span className="text-xs uppercase font-semibold px-1.5 py-0.5 rounded mt-0.5 bg-emerald-500/10 text-emerald-400 font-display">
                        FN
                      </span>
                      <div className="min-w-0">
                        <div className="font-mono text-sm text-white truncate">{highlightMatch(m.name, query)}</div>
                        <div className="text-xs text-slate-400 truncate">{highlightMatch(m.description, query)}</div>
                      </div>
                    </button>
                  );
                })}
              </>
            )}
            {typeResults.length > 0 && (
              <>
                <div className="px-4 pt-3 pb-1 text-xs uppercase tracking-wider text-slate-400 font-semibold font-display">
                  Types
                </div>
                {typeResults.slice(0, 10).map((t) => {
                  const globalIdx = allResults.findIndex(r => r.type === 'type' && r.id === t.id);
                  return (
                    <button
                      key={`type-${t.id}`}
                      onClick={() => go({ type: 'type', id: t.id })}
                      className={`w-full text-left px-4 py-2 transition-colors flex items-start gap-3 ${
                        globalIdx === selectedIdx ? 'bg-slate-800/80' : 'hover:bg-slate-800/60'
                      }`}
                    >
                      <span className="text-xs uppercase font-semibold px-1.5 py-0.5 rounded mt-0.5 bg-sky-500/10 text-sky-400 font-display">
                        T
                      </span>
                      <div className="min-w-0">
                        <div className="font-mono text-sm text-white truncate">{highlightMatch(t.name, query)}</div>
                        <div className="text-xs text-slate-400 truncate">{highlightMatch(t.description, query)}</div>
                      </div>
                    </button>
                  );
                })}
              </>
            )}
          </div>
        )}

        {query.length > 0 && allResults.length === 0 && (
          <div className="py-8 text-center text-sm text-slate-500">No results found</div>
        )}

        {query.length === 0 && (
          <div className="py-6 text-center text-sm text-slate-500">
            <p>Start typing to search {methods.length} methods and {dataTypes.length} types</p>
            <div className="flex items-center justify-center gap-3 mt-2 text-xs text-slate-500">
              <span><kbd className="bg-slate-800 border border-slate-700/50 px-1 py-0.5 rounded font-mono">↑↓</kbd> Navigate</span>
              <span><kbd className="bg-slate-800 border border-slate-700/50 px-1 py-0.5 rounded font-mono">↵</kbd> Open</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function PageTransition({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  return (
    <div key={location.pathname} className="animate-fade-in" style={{ position: 'relative', zIndex: 1 }}>
      {children}
    </div>
  );
}

function VersionedApp() {
  const { version: vSlug } = useParams<{ version: string }>();
  const versionMeta = getVersion(vSlug || '');

  if (!versionMeta) {
    return <Navigate to={`/v/${defaultVersion.slug}/`} replace />;
  }

  return (
    <VersionProvider version={versionMeta}>
      <VersionedAppInner />
    </VersionProvider>
  );
}

const SIDEBAR_MIN_WIDTH = 200;
const SIDEBAR_MAX_WIDTH = 500;
const SIDEBAR_DEFAULT_WIDTH = 288;
const SIDEBAR_WIDTH_KEY = 'sidebarWidth';

function VersionedAppInner() {
  const [searchOpen, setSearchOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState<number>(() => {
    if (typeof window === 'undefined') return SIDEBAR_DEFAULT_WIDTH;
    const stored = Number(window.localStorage.getItem(SIDEBAR_WIDTH_KEY));
    if (!Number.isFinite(stored) || stored <= 0) return SIDEBAR_DEFAULT_WIDTH;
    return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, stored));
  });
  const resizeStateRef = useRef<{ startX: number; startWidth: number } | null>(null);

  useEffect(() => {
    try { window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth)); } catch { /* ignore */ }
  }, [sidebarWidth]);

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

  const handleResizePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    resizeStateRef.current = { startX: e.clientX, startWidth: sidebarWidth };
    document.body.style.userSelect = 'none';
  };

  const handleResizePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!resizeStateRef.current) return;
    const { startX, startWidth } = resizeStateRef.current;
    const next = Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, startWidth + (e.clientX - startX)));
    setSidebarWidth(next);
  };

  const endResize = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!resizeStateRef.current) return;
    try { e.currentTarget.releasePointerCapture(e.pointerId); } catch { /* ignore */ }
    resizeStateRef.current = null;
    document.body.style.userSelect = '';
  };

  return (
    <div className="flex min-h-screen bg-slate-925">
      <Sidebar open={sidebarOpen} onClose={() => setSidebarOpen(false)} width={sidebarWidth} />

      <div
        role="separator"
        aria-orientation="vertical"
        className="hidden lg:block sticky top-0 h-screen w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-pink-500/60 active:bg-pink-500/80 transition-colors z-10"
        onPointerDown={handleResizePointerDown}
        onPointerMove={handleResizePointerMove}
        onPointerUp={endResize}
        onPointerCancel={endResize}
        onDoubleClick={() => setSidebarWidth(SIDEBAR_DEFAULT_WIDTH)}
      />

      <main className="flex-1 min-w-0">
        {/* Top bar */}
        <div className="sticky top-0 z-10 bg-slate-925/80 backdrop-blur-md border-b border-slate-700/40">
          <div className="flex items-center justify-between px-4 lg:px-8 py-3">
            <div className="flex items-center gap-3">
              <button
                onClick={() => setSidebarOpen(true)}
                className="lg:hidden p-1.5 text-slate-400 hover:text-white transition-colors"
              >
                <Menu size={20} />
              </button>
              <button
                onClick={() => setSearchOpen(true)}
                className="flex items-center gap-2 bg-slate-800/50 border border-slate-700/40 rounded-lg px-3 py-1.5 text-sm text-slate-400 hover:text-slate-200 hover:border-slate-600/50 transition-all duration-150 hover:shadow-[0_2px_8px_rgba(0,0,0,0.2)]"
              >
                <Search size={14} />
                <span>Search...</span>
                <kbd className="text-xs text-slate-400 bg-slate-700/50 px-1.5 py-0.5 rounded ml-4 font-mono hidden sm:inline">
                  {navigator.platform.includes('Mac') ? '\u2318' : 'Ctrl'}K
                </kbd>
              </button>
            </div>
          </div>
        </div>

        {/* Content */}
        <div className="px-4 py-6 lg:px-8 lg:py-8">
          <PageTransition>
            <Routes>
              <Route path="/" element={<OverviewPage />} />
              <Route path="/method/:id" element={<MethodPage />} />
              <Route path="/types" element={<TypesPage />} />
              <Route path="/type/:id" element={<TypeDetailPage />} />
            </Routes>
          </PageTransition>
        </div>
      </main>

      <SearchModal open={searchOpen} onClose={() => setSearchOpen(false)} />
    </div>
  );
}

function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to={`/v/${defaultVersion.slug}/`} replace />} />
      <Route path="/v/:version/*" element={<VersionedApp />} />
      {/* Legacy redirects */}
      <Route path="/method/:id" element={<LegacyRedirect />} />
      <Route path="/types" element={<Navigate to={`/v/${defaultVersion.slug}/types`} replace />} />
      <Route path="/type/:id" element={<LegacyRedirect />} />
      <Route path="*" element={<Navigate to={`/v/${defaultVersion.slug}/`} replace />} />
    </Routes>
  );
}

function LegacyRedirect() {
  const location = useLocation();
  return <Navigate to={`/v/${defaultVersion.slug}${location.pathname}`} replace />;
}

export default App;
