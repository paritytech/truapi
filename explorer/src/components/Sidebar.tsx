import { useEffect, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { ChevronDown, X } from "lucide-react";
import VersionSelector from "./VersionSelector";
import { methodPath } from "../data/registry";
import type { VersionEntry } from "../data/types";

interface SidebarProps {
  open: boolean;
  onClose: () => void;
  width: number;
  versions: VersionEntry[];
  current: VersionEntry;
}

/** Left navigation: wordmark, version selector, top-level links, service tree. */
export default function Sidebar({
  open,
  onClose,
  width,
  versions,
  current,
}: SidebarProps) {
  const location = useLocation();
  const prefix = `/v/${current.id}`;
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set(current.services.map((s) => s.name)),
  );

  useEffect(() => {
    setExpanded(new Set(current.services.map((s) => s.name)));
  }, [current]);

  const toggle = (name: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const isActive = (path: string) =>
    location.pathname === path || location.pathname === path + "/";

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
        className={`
          fixed inset-y-0 left-0 z-50 border-r border-slate-700/50 bg-slate-925 flex flex-col h-screen overflow-hidden
          transform transition-transform duration-300 ease-in-out
          ${open ? "translate-x-0" : "-translate-x-full"}
          lg:translate-x-0 lg:sticky lg:top-0 lg:z-auto
        `}
      >
        <div className="p-4 border-b border-slate-700/50">
          <div className="flex items-center justify-between">
            <Link to={`${prefix}/`} className="flex items-center gap-2.5 group">
              <div className="w-8 h-8 rounded-lg bg-pink-600 flex items-center justify-center group-hover:shadow-[0_0_12px_rgba(219,39,119,0.4)] transition-shadow">
                <span className="text-white text-sm font-bold font-display">
                  T
                </span>
              </div>
              <div>
                <h1 className="text-sm font-semibold text-white leading-tight font-display tracking-tight">
                  TrUAPI Explorer
                </h1>
              </div>
            </Link>
            <button
              onClick={onClose}
              className="lg:hidden p-1 text-slate-400 hover:text-white transition-colors"
            >
              <X size={18} />
            </button>
          </div>

          <div className="mt-3">
            <VersionSelector versions={versions} current={current} />
          </div>
        </div>

        <div className="px-3 pt-3 pb-1">
          <Link
            to={`${prefix}/`}
            onClick={onClose}
            className={`block px-3 py-1.5 rounded-md text-sm transition-all duration-150 ${
              isActive(prefix)
                ? "bg-slate-800 text-white font-medium"
                : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            }`}
          >
            Overview
          </Link>
          <Link
            to={`${prefix}/types`}
            onClick={onClose}
            className={`block px-3 py-1.5 rounded-md text-sm transition-all duration-150 ${
              location.pathname === `${prefix}/types` ||
              location.pathname.startsWith(`${prefix}/type/`)
                ? "bg-slate-800 text-white font-medium"
                : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            }`}
          >
            Data Types
          </Link>
          <Link
            to={`${prefix}/compatibility`}
            onClick={onClose}
            className={`block px-3 py-1.5 rounded-md text-sm transition-all duration-150 ${
              isActive(`${prefix}/compatibility`)
                ? "bg-slate-800 text-white font-medium"
                : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
            }`}
          >
            Compatibility
          </Link>
        </div>

        <nav className="flex-1 overflow-y-auto px-3 pb-4 pt-1">
          <div className="text-xs uppercase tracking-wider text-slate-400 font-semibold px-3 mb-2 mt-2 font-display">
            Services
          </div>
          {current.services.map((service) => {
            const isExpanded = expanded.has(service.name);
            const hasActive = service.methods.some(
              (m) =>
                location.pathname ===
                methodPath(current.id, service.name, m.name),
            );
            return (
              <div key={service.name} className="mb-0.5">
                <button
                  onClick={() => toggle(service.name)}
                  className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-colors ${
                    hasActive
                      ? "text-white bg-slate-800/30"
                      : "text-slate-300 hover:text-white hover:bg-slate-800/50"
                  }`}
                >
                  <span className="flex-1 text-left truncate">
                    {service.name}
                  </span>
                  <span
                    className={`transition-transform duration-200 ${isExpanded ? "rotate-180" : ""}`}
                  >
                    <ChevronDown size={14} className="text-slate-500" />
                  </span>
                </button>
                {isExpanded && (
                  <div className="ml-3 border-l border-slate-700/50 pl-2 py-0.5">
                    {service.methods.map((method) => {
                      const path = methodPath(
                        current.id,
                        service.name,
                        method.name,
                      );
                      const active = location.pathname === path;
                      return (
                        <Link
                          key={method.name}
                          to={path}
                          onClick={onClose}
                          className={`block px-2 py-1 rounded text-xs font-mono truncate transition-all duration-150 ${
                            active
                              ? "bg-pink-500/15 text-pink-300 font-medium shadow-[inset_3px_0_0_0_var(--color-pink-500)] -ml-[1px] pl-[9px]"
                              : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/30"
                          }`}
                        >
                          {method.name}
                        </Link>
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
