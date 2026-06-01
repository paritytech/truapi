import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { ChevronDown } from "lucide-react";
import type { VersionEntry } from "../data/types";
import { findMethod, findType, methodPath } from "../data/registry";

interface VersionSelectorProps {
  versions: VersionEntry[];
  current: VersionEntry;
}

/** Drop-down for switching the active protocol version. */
export default function VersionSelector({
  versions,
  current,
}: VersionSelectorProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const location = useLocation();

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node))
        setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const switchTo = (target: VersionEntry) => {
    setOpen(false);
    if (target.id === current.id) return;

    const path = location.pathname;
    const prefix = `/v/${current.id}`;
    const sub = path.startsWith(prefix) ? path.slice(prefix.length) : "/";
    const nextPrefix = `/v/${target.id}`;

    const methodMatch = sub.match(/^\/method\/([^/]+)\/(.+)$/);
    if (methodMatch) {
      const serviceName = decodeURIComponent(methodMatch[1]);
      const methodName = decodeURIComponent(methodMatch[2]);
      if (findMethod(target, serviceName, methodName)) {
        navigate(methodPath(target.id, serviceName, methodName));
        return;
      }
      navigate(`${nextPrefix}/`);
      return;
    }
    const typeMatch = sub.match(/^\/type\/(.+)$/);
    if (typeMatch) {
      const id = typeMatch[1];
      if (findType(target, id)) {
        navigate(`${nextPrefix}/type/${id}`);
        return;
      }
      navigate(`${nextPrefix}/types`);
      return;
    }
    if (sub === "/types") {
      navigate(`${nextPrefix}/types`);
      return;
    }
    navigate(`${nextPrefix}/`);
  };

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center justify-between px-2.5 py-1.5 rounded-md bg-slate-800/60 border border-slate-700/50 text-xs text-slate-300 hover:border-slate-600/60 transition-colors"
      >
        <span className="flex items-center gap-1.5">
          <span className="w-1.5 h-1.5 rounded-full bg-pink-400" />
          Version <span className="font-mono">{current.id}</span>
        </span>
        <ChevronDown
          size={12}
          className={`text-slate-500 transition-transform duration-200 ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="absolute top-full left-0 right-0 mt-1 bg-slate-800 border border-slate-700/60 rounded-md shadow-xl z-20 overflow-hidden animate-scale-in">
          {versions.map((v) => (
            <button
              key={v.id}
              onClick={() => switchTo(v)}
              className={`w-full text-left px-3 py-2 text-xs hover:bg-slate-700/50 transition-colors flex items-center justify-between ${
                v.id === current.id
                  ? "text-white bg-slate-700/30"
                  : "text-slate-400"
              }`}
            >
              <span className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-pink-400" />
                <span className="font-mono">{v.id}</span>
              </span>
              <span className="text-[10px] text-slate-500">
                {v.services.length} svc
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
