import { useLocation, useNavigate } from 'react-router-dom';
import { groups, methods } from '../data/types';
import {
  ChevronDown,
  Zap,
  Shield,
  HardDrive,
  User,
  PenTool,
  MessageSquare,
  FileText,
  Image,
  Link,
  Database,
  X,
} from 'lucide-react';
import { useState, useEffect, useRef } from 'react';

const groupIcons: Record<string, React.ReactNode> = {
  'host-calls': <Zap size={15} />,
  'permissions': <Shield size={15} />,
  'local-storage': <HardDrive size={15} />,
  'account-management': <User size={15} />,
  'signing': <PenTool size={15} />,
  'chat': <MessageSquare size={15} />,
  'statement-store': <FileText size={15} />,
  'preimage': <Image size={15} />,
  'chain-interaction': <Link size={15} />,
};

const PROTOCOL_VERSIONS = [
  { id: 'v0.1', label: 'v0.1', current: true },
];

export default function Sidebar({ open, onClose }: { open: boolean; onClose: () => void }) {
  const location = useLocation();
  const navigate = useNavigate();
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(
    new Set(groups.map(g => g.id))
  );
  const [versionOpen, setVersionOpen] = useState(false);
  const [selectedVersion] = useState('v0.1');
  const versionRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!versionOpen) return;
    const handler = (e: MouseEvent) => {
      if (versionRef.current && !versionRef.current.contains(e.target as Node)) {
        setVersionOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [versionOpen]);

  const toggleGroup = (id: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const currentPath = location.pathname;

  const handleNav = (path: string) => {
    navigate(path);
    onClose();
  };

  return (
    <>
      {/* Mobile backdrop */}
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm lg:hidden"
          onClick={onClose}
        />
      )}
      <aside className={`
        fixed inset-y-0 left-0 z-50 w-72 min-w-72 border-r border-slate-700/50 bg-slate-925 flex flex-col h-screen overflow-hidden
        transform transition-transform duration-300 ease-in-out
        ${open ? 'translate-x-0' : '-translate-x-full'}
        lg:translate-x-0 lg:static lg:z-auto
      `}>
      {/* Header */}
      <div className="p-4 border-b border-slate-700/50">
        <div className="flex items-center justify-between">
        <div
          className="flex items-center gap-2.5 cursor-pointer group"
          onClick={() => handleNav('/')}
        >
          <div className="w-8 h-8 rounded-lg bg-pink-600 flex items-center justify-center group-hover:shadow-[0_0_12px_rgba(219,39,119,0.4)] transition-shadow">
            <span className="text-white text-sm font-bold font-display">H</span>
          </div>
          <div>
            <h1 className="text-sm font-semibold text-white leading-tight font-display tracking-tight">Host API</h1>
          </div>
        </div>
        <button
          onClick={onClose}
          className="lg:hidden p-1 text-slate-400 hover:text-white transition-colors"
        >
          <X size={18} />
        </button>
        </div>

        {/* Version selector */}
        <div className="mt-3 relative" ref={versionRef}>
          <button
            onClick={() => setVersionOpen(!versionOpen)}
            className="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md bg-slate-800/60 border border-slate-700/50 text-xs text-slate-300 hover:border-slate-600/60 transition-colors"
          >
            <span className="flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
              Protocol {selectedVersion}
            </span>
            <ChevronDown size={12} className={`text-slate-500 transition-transform duration-200 ${versionOpen ? 'rotate-180' : ''}`} />
          </button>
          {versionOpen && (
            <div className="absolute top-full left-0 right-0 mt-1 bg-slate-800 border border-slate-700/60 rounded-md shadow-xl z-20 overflow-hidden animate-scale-in">
              {PROTOCOL_VERSIONS.map(v => (
                <button
                  key={v.id}
                  onClick={() => setVersionOpen(false)}
                  className={`w-full text-left px-3 py-2 text-xs hover:bg-slate-700/50 transition-colors flex items-center justify-between ${
                    v.id === selectedVersion ? 'text-white bg-slate-700/30' : 'text-slate-400'
                  }`}
                >
                  <span className="flex items-center gap-1.5">
                    <span className={`w-1.5 h-1.5 rounded-full ${v.current ? 'bg-emerald-400' : 'bg-slate-500'}`} />
                    {v.label}
                  </span>
                  {v.current && <span className="text-[9px] text-emerald-400 font-medium">CURRENT</span>}
                </button>
              ))}
              <div className="px-3 py-2 border-t border-slate-700/40 text-[10px] text-slate-500">
                More versions coming soon
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Nav links */}
      <div className="px-3 pt-3 pb-1">
        <button
          onClick={() => handleNav('/')}
          className={`w-full text-left px-3 py-1.5 rounded-md text-sm transition-all duration-150 ${
            currentPath === '/' ? 'bg-slate-800 text-white font-medium' : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
          }`}
        >
          Overview
        </button>
        <button
          onClick={() => handleNav('/types')}
          className={`w-full text-left px-3 py-1.5 rounded-md text-sm transition-all duration-150 flex items-center gap-2 ${
            currentPath === '/types' || currentPath.startsWith('/type/') ? 'bg-slate-800 text-white font-medium' : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
          }`}
        >
          <Database size={14} />
          Data Types
        </button>
      </div>

      {/* Groups */}
      <nav className="flex-1 overflow-y-auto px-3 pb-4 pt-1">
        <div className="text-xs uppercase tracking-wider text-slate-400 font-semibold px-3 mb-2 mt-2 font-display">
          Methods
        </div>
        {groups.map(group => {
          const isExpanded = expandedGroups.has(group.id);
          const groupMethods = methods.filter(m => m.groupId === group.id);
          const hasActive = groupMethods.some(m => currentPath === `/method/${m.id}`);

          return (
            <div key={group.id} className="mb-0.5">
              <button
                onClick={() => toggleGroup(group.id)}
                className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-colors ${
                  hasActive
                    ? 'text-white bg-slate-800/30'
                    : 'text-slate-300 hover:text-white hover:bg-slate-800/50'
                }`}
              >
                <span className={`transition-colors ${hasActive ? 'text-pink-400' : 'text-slate-500'}`}>
                  {groupIcons[group.id]}
                </span>
                <span className="flex-1 text-left truncate">{group.name}</span>
                <span className={`transition-transform duration-200 ${isExpanded ? 'rotate-180' : ''}`}>
                  <ChevronDown size={14} className="text-slate-500" />
                </span>
              </button>

              {isExpanded && (
                <div className="ml-4 border-l border-slate-700/50 pl-2 animate-slide-down">
                  {groupMethods.map(method => {
                    const isActive = currentPath === `/method/${method.id}`;
                    return (
                      <button
                        key={method.id}
                        onClick={() => handleNav(`/method/${method.id}`)}
                        className={`w-full text-left px-2 py-1 rounded text-xs font-mono truncate transition-all duration-150 ${
                          isActive
                            ? 'bg-pink-500/15 text-pink-300 font-medium shadow-[inset_3px_0_0_0_theme(colors.pink.500)] -ml-[1px] pl-[9px]'
                            : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/30'
                        }`}
                      >
                        {method.name}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </nav>

      {/* Footer */}
      <div className="p-3 border-t border-slate-700/50 text-[10px] text-slate-500">
        <span className="font-mono">@novasamatech/host-api</span>
      </div>
    </aside>
    </>
  );
}
