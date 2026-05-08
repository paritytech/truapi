import { useEffect, useRef, useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  ChevronDown,
  Database,
  FileText,
  HardDrive,
  Image,
  Key,
  Link,
  MessageSquare,
  PenTool,
  Shield,
  User,
  Wallet,
  X,
  Zap,
} from "lucide-react";
import { versions } from "../data/registry";
import { useVersion } from "../contexts/VersionContext";

const groupIcons: Record<string, ReactNode> = {
  "truapi-calls": <Zap size={15} />,
  permissions: <Shield size={15} />,
  "local-storage": <HardDrive size={15} />,
  "account-management": <User size={15} />,
  signing: <PenTool size={15} />,
  chat: <MessageSquare size={15} />,
  "statement-store": <FileText size={15} />,
  preimage: <Image size={15} />,
  "chain-interaction": <Link size={15} />,
  payment: <Wallet size={15} />,
  "entropy-derivation": <Key size={15} />,
};

export default function Sidebar({
  open,
  onClose,
  width = 288,
}: {
  open: boolean;
  onClose: () => void;
  width?: number;
}) {
  const location = useLocation();
  const navigate = useNavigate();
  const { groups, methods, version, versionPrefix } = useVersion();
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(
    new Set(groups.map((g) => g.id)),
  );
  const [versionOpen, setVersionOpen] = useState(false);
  const versionRef = useRef<HTMLDivElement>(null);

  useEffect(
    () => setExpandedGroups(new Set(groups.map((g) => g.id))),
    [groups],
  );
  useEffect(() => {
    if (!versionOpen) return;
    const handler = (e: MouseEvent) => {
      if (
        versionRef.current &&
        !versionRef.current.contains(e.target as Node)
      ) {
        setVersionOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [versionOpen]);

  const handleNav = (path: string) => {
    navigate(path);
    onClose();
  };
  const currentPath = location.pathname;

  return (
    <>
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm lg:hidden"
          onClick={onClose}
        />
      )}
      <aside
        style={{ width, minWidth: width }}
        className={`fixed inset-y-0 left-0 z-50 border-r border-slate-700/50 bg-slate-925 flex flex-col h-screen overflow-hidden transform transition-transform duration-300 ease-in-out ${
          open ? "translate-x-0" : "-translate-x-full"
        } lg:translate-x-0 lg:sticky lg:top-0 lg:z-auto`}
      >
        <div className="p-4 border-b border-slate-700/50">
          <div className="flex items-center justify-between">
            <button
              className="flex items-center gap-2.5 group"
              onClick={() => handleNav(versionPrefix + "/")}
              type="button"
            >
              <div className="w-8 h-8 rounded-lg bg-pink-600 flex items-center justify-center group-hover:shadow-[0_0_12px_rgba(219,39,119,0.4)] transition-shadow">
                <span className="text-white text-sm font-bold font-display">
                  T
                </span>
              </div>
              <h1 className="text-sm font-semibold text-white leading-tight font-display tracking-tight">
                TrUAPI
              </h1>
            </button>
            <button
              onClick={onClose}
              className="lg:hidden p-1 text-slate-400 hover:text-white transition-colors"
              type="button"
            >
              <X size={18} />
            </button>
          </div>

          <div className="mt-3 relative" ref={versionRef}>
            <button
              onClick={() => setVersionOpen(!versionOpen)}
              className="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md bg-slate-800/60 border border-slate-700/50 text-xs text-slate-300 hover:border-slate-600/60 transition-colors"
              type="button"
            >
              <span className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                Protocol {version.label}
              </span>
              <ChevronDown
                size={12}
                className={`text-slate-500 transition-transform duration-200 ${
                  versionOpen ? "rotate-180" : ""
                }`}
              />
            </button>
            {versionOpen && (
              <div className="absolute top-full left-0 right-0 mt-1 bg-slate-800 border border-slate-700/60 rounded-md shadow-xl z-20 overflow-hidden animate-scale-in">
                {versions.map((v) => (
                  <button
                    key={v.id}
                    onClick={() => {
                      navigate(`/v/${v.slug}/`);
                      setVersionOpen(false);
                    }}
                    className={`w-full text-left px-3 py-2 text-xs hover:bg-slate-700/50 transition-colors flex items-center justify-between ${
                      v.id === version.id
                        ? "text-white bg-slate-700/30"
                        : "text-slate-400"
                    }`}
                    type="button"
                  >
                    <span className="flex items-center gap-1.5">
                      <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                      {v.label}
                    </span>
                    <span className="text-[9px] text-emerald-400 font-medium">
                      STABLE
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="px-3 pt-3 pb-1">
          <button
            onClick={() => handleNav(versionPrefix + "/")}
            className={`w-full text-left px-3 py-1.5 rounded-md text-sm transition-all duration-150 ${
              currentPath === versionPrefix + "/" ||
              currentPath === versionPrefix
                ? "bg-slate-800 text-white font-medium"
                : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            }`}
            type="button"
          >
            Overview
          </button>
          <button
            onClick={() => handleNav(versionPrefix + "/types")}
            className={`w-full text-left px-3 py-1.5 rounded-md text-sm transition-all duration-150 flex items-center gap-2 ${
              currentPath === versionPrefix + "/types" ||
              currentPath.startsWith(versionPrefix + "/type/")
                ? "bg-slate-800 text-white font-medium"
                : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            }`}
            type="button"
          >
            <Database size={14} />
            Data Types
          </button>
        </div>

        <nav className="flex-1 overflow-y-auto px-3 pb-4 pt-1">
          <div className="text-xs uppercase tracking-wider text-slate-400 font-semibold px-3 mb-2 mt-2 font-display">
            Methods
          </div>
          {groups.map((group) => {
            const isExpanded = expandedGroups.has(group.id);
            const groupMethods = methods.filter((m) => m.groupId === group.id);
            const hasActive = groupMethods.some(
              (m) => currentPath === `${versionPrefix}/method/${m.id}`,
            );
            return (
              <div key={group.id} className="mb-0.5">
                <button
                  onClick={() =>
                    setExpandedGroups((prev) => {
                      const next = new Set(prev);
                      if (next.has(group.id)) next.delete(group.id);
                      else next.add(group.id);
                      return next;
                    })
                  }
                  className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-colors ${
                    hasActive
                      ? "text-white bg-slate-800/30"
                      : "text-slate-300 hover:text-white hover:bg-slate-800/50"
                  }`}
                  type="button"
                >
                  <span
                    className={`transition-colors ${
                      hasActive ? "text-pink-400" : "text-slate-500"
                    }`}
                  >
                    {groupIcons[group.id]}
                  </span>
                  <span className="flex-1 text-left truncate">
                    {group.name}
                  </span>
                  <ChevronDown
                    size={14}
                    className={`text-slate-500 transition-transform duration-200 ${
                      isExpanded ? "rotate-180" : ""
                    }`}
                  />
                </button>
                {isExpanded && (
                  <div className="ml-4 border-l border-slate-700/50 pl-2 animate-slide-down">
                    {groupMethods.map((method) => {
                      const isActive =
                        currentPath === `${versionPrefix}/method/${method.id}`;
                      return (
                        <button
                          key={method.id}
                          onClick={() =>
                            handleNav(`${versionPrefix}/method/${method.id}`)
                          }
                          className={`w-full text-left px-2 py-1 rounded text-xs font-mono truncate transition-all duration-150 ${
                            isActive
                              ? "bg-pink-500/15 text-pink-300 font-medium shadow-[inset_3px_0_0_0_theme(colors.pink.500)] -ml-[1px] pl-[9px]"
                              : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/30"
                          }`}
                          type="button"
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

        <div className="p-3 border-t border-slate-700/50 text-[10px] text-slate-500">
          <span className="font-mono">@parity/truapi</span>
        </div>
      </aside>
    </>
  );
}
