import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Search } from "lucide-react";
import type {
  DataType,
  MethodInfo,
  ServiceInfo,
  VersionEntry,
} from "../data/types";

interface SearchModalProps {
  open: boolean;
  onClose: () => void;
  version: VersionEntry;
}

interface MethodHit {
  kind: "method";
  service: ServiceInfo;
  method: MethodInfo;
}

interface TypeHit {
  kind: "type";
  type: DataType;
}

type Hit = MethodHit | TypeHit;

function matches(
  needle: string,
  ...haystacks: (string | undefined)[]
): boolean {
  const q = needle.toLowerCase();
  for (const h of haystacks) {
    if (h && h.toLowerCase().includes(q)) return true;
  }
  return false;
}

function highlight(text: string, query: string): React.ReactNode {
  if (!query) return text;
  const idx = text.toLowerCase().indexOf(query.toLowerCase());
  if (idx === -1) return text;
  return (
    <>
      {text.slice(0, idx)}
      <span className="text-white bg-pink-500/20 rounded px-0.5">
        {text.slice(idx, idx + query.length)}
      </span>
      {text.slice(idx + query.length)}
    </>
  );
}

/** Cmd/Ctrl-K command palette. */
export default function SearchModal({
  open,
  onClose,
  version,
}: SearchModalProps) {
  const [query, setQuery] = useState("");
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  const { methodHits, typeHits, hits } = useMemo(() => {
    if (query.length === 0) {
      return {
        methodHits: [] as MethodHit[],
        typeHits: [] as TypeHit[],
        hits: [] as Hit[],
      };
    }
    const mh: MethodHit[] = [];
    const th: TypeHit[] = [];
    for (const service of version.services) {
      for (const method of service.methods) {
        if (matches(query, method.name, method.description)) {
          mh.push({ kind: "method", service, method });
        }
      }
    }
    for (const t of version.types) {
      if (matches(query, t.name, t.description, t.category)) {
        th.push({ kind: "type", type: t });
      }
    }
    const mhTrimmed = mh.slice(0, 10);
    const thTrimmed = th.slice(0, 10);
    const all: Hit[] = [...mhTrimmed, ...thTrimmed];
    return { methodHits: mhTrimmed, typeHits: thTrimmed, hits: all };
  }, [query, version]);

  const go = (hit: Hit) => {
    if (hit.kind === "method") {
      navigate(`/v/${version.id}/method/${hit.method.name}`);
    } else {
      navigate(`/v/${version.id}/type/${hit.type.id}`);
    }
    onClose();
  };

  if (!open) return null;

  const totalMethods = version.services.reduce(
    (acc, s) => acc + s.methods.length,
    0,
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]"
      onClick={onClose}
    >
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-fade-in" />
      <div
        className="relative bg-slate-850 border border-slate-700/60 rounded-xl w-full max-w-xl shadow-2xl overflow-hidden animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-700/40">
          <Search size={18} className="text-slate-400" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Search methods and types..."
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIdx(0);
            }}
            className="flex-1 bg-transparent text-white placeholder:text-slate-500 focus:outline-none text-sm"
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelectedIdx((i) => Math.min(i + 1, hits.length - 1));
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelectedIdx((i) => Math.max(i - 1, 0));
              }
              if (e.key === "Enter" && hits.length > 0) {
                go(hits[selectedIdx]);
              }
            }}
          />
          <kbd className="text-xs text-slate-400 bg-slate-800 border border-slate-700/50 px-1.5 py-0.5 rounded font-mono">
            ESC
          </kbd>
        </div>

        {query.length > 0 && hits.length > 0 && (
          <div className="max-h-80 overflow-y-auto py-1">
            {methodHits.length > 0 && (
              <>
                <div className="px-4 pt-2 pb-1 text-xs uppercase tracking-wider text-slate-400 font-semibold font-display">
                  Methods
                </div>
                {methodHits.map((hit) => {
                  const globalIdx = hits.indexOf(hit);
                  return (
                    <button
                      key={`m-${hit.service.name}-${hit.method.name}`}
                      onClick={() => go(hit)}
                      className={`w-full text-left px-4 py-2 transition-colors flex items-start gap-3 ${
                        globalIdx === selectedIdx
                          ? "bg-slate-800/80"
                          : "hover:bg-slate-800/60"
                      }`}
                    >
                      <span className="text-xs uppercase font-semibold px-1.5 py-0.5 rounded mt-0.5 bg-emerald-500/10 text-emerald-400 font-display">
                        FN
                      </span>
                      <div className="min-w-0">
                        <div className="font-mono text-sm text-white truncate">
                          {highlight(hit.method.name, query)}
                        </div>
                        <div className="text-xs text-slate-400 truncate">
                          {hit.service.name}
                          {hit.method.description && (
                            <>
                              {" · "}
                              {highlight(hit.method.description, query)}
                            </>
                          )}
                        </div>
                      </div>
                    </button>
                  );
                })}
              </>
            )}
            {typeHits.length > 0 && (
              <>
                <div className="px-4 pt-3 pb-1 text-xs uppercase tracking-wider text-slate-400 font-semibold font-display">
                  Types
                </div>
                {typeHits.map((hit) => {
                  const globalIdx = hits.indexOf(hit);
                  return (
                    <button
                      key={`t-${hit.type.id}`}
                      onClick={() => go(hit)}
                      className={`w-full text-left px-4 py-2 transition-colors flex items-start gap-3 ${
                        globalIdx === selectedIdx
                          ? "bg-slate-800/80"
                          : "hover:bg-slate-800/60"
                      }`}
                    >
                      <span className="text-xs uppercase font-semibold px-1.5 py-0.5 rounded mt-0.5 bg-sky-500/10 text-sky-400 font-display">
                        T
                      </span>
                      <div className="min-w-0">
                        <div className="font-mono text-sm text-white truncate">
                          {highlight(hit.type.name, query)}
                        </div>
                        <div className="text-xs text-slate-400 truncate">
                          {hit.type.category}
                          {hit.type.description && (
                            <>
                              {" · "}
                              {highlight(hit.type.description, query)}
                            </>
                          )}
                        </div>
                      </div>
                    </button>
                  );
                })}
              </>
            )}
          </div>
        )}

        {query.length > 0 && hits.length === 0 && (
          <div className="py-8 text-center text-sm text-slate-500">
            No results found
          </div>
        )}

        {query.length === 0 && (
          <div className="py-6 text-center text-sm text-slate-500">
            <p>
              Start typing to search {totalMethods} methods and{" "}
              {version.types.length} types
            </p>
            <div className="flex items-center justify-center gap-3 mt-2 text-xs text-slate-500">
              <span>
                <kbd className="bg-slate-800 border border-slate-700/50 px-1 py-0.5 rounded font-mono">
                  Up Down
                </kbd>{" "}
                Navigate
              </span>
              <span>
                <kbd className="bg-slate-800 border border-slate-700/50 px-1 py-0.5 rounded font-mono">
                  Enter
                </kbd>{" "}
                Open
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
