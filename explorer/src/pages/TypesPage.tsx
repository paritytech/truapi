import { useMemo, useState } from "react";
import { Link, useOutletContext } from "react-router-dom";
import { Search } from "lucide-react";
import type { VersionEntry } from "../data/types";
import { titleCase } from "../data/registry";

/** Types index: search bar + sections grouped by category. */
export default function TypesPage() {
  const { version } = useOutletContext<{ version: VersionEntry }>();
  const [query, setQuery] = useState("");

  const grouped = useMemo(() => {
    const q = query.toLowerCase();
    const filtered = query
      ? version.types.filter(
          (t) =>
            t.name.toLowerCase().includes(q) ||
            (t.description ?? "").toLowerCase().includes(q) ||
            t.category.toLowerCase().includes(q),
        )
      : version.types;

    const map = new Map<string, typeof filtered>();
    for (const t of filtered) {
      const cat = t.category;
      const bucket = map.get(cat) ?? [];
      bucket.push(t);
      map.set(cat, bucket);
    }
    return Array.from(map.entries()).map(
      ([category, types]) =>
        ({
          category,
          types: types.sort((a, b) => a.name.localeCompare(b.name)),
        }) as const,
    );
  }, [version.types, query]);

  return (
    <div className="max-w-5xl mx-auto">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2 mb-8 animate-slide-up">
        <div>
          <h1 className="text-2xl font-bold text-white mb-1 font-display tracking-tight">
            Data Types
          </h1>
          <p className="text-sm text-slate-400">
            Shared types used across the TrUAPI service surface.
          </p>
        </div>
        <div className="text-sm text-slate-400 font-display">
          {version.types.length} types
        </div>
      </div>

      <div className="relative mb-8 animate-slide-up">
        <Search
          size={16}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500"
        />
        <input
          type="text"
          placeholder="Search types..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-full bg-slate-800/50 border border-slate-700/50 rounded-lg pl-10 pr-4 py-2.5 text-sm text-white placeholder:text-slate-500 focus:outline-none focus:border-pink-500/50 focus:ring-1 focus:ring-pink-500/20 transition-all duration-200"
        />
      </div>

      {grouped.map(({ category, types }) => (
        <div key={category} className="mb-8 animate-slide-up">
          <h2 className="text-sm font-semibold text-slate-300 uppercase tracking-wider mb-3 flex items-center gap-2 font-display">
            <span className="w-2 h-2 rounded-full bg-pink-500/60" />
            {titleCase(category)}
            <span className="text-xs font-normal text-slate-400 normal-case">
              ({types.length})
            </span>
          </h2>
          <div className="grid grid-cols-1 gap-1.5">
            {types.map((t) => (
              <Link
                key={t.id}
                to={`/v/${version.id}/type/${t.id}`}
                className="bg-slate-800/30 border border-slate-700/30 rounded-lg px-4 py-3 hover:border-slate-600/50 hover:bg-slate-800/50 transition-all duration-150 group hover:shadow-[0_4px_12px_rgba(0,0,0,0.2)]"
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0">
                    <span className="font-mono text-sm text-sky-400 group-hover:text-sky-300 transition-colors">
                      {t.name}
                    </span>
                    {t.description && (
                      <p className="text-sm text-slate-400 mt-0.5 truncate">
                        {t.description}
                      </p>
                    )}
                  </div>
                  <code className="text-xs font-mono text-slate-400 shrink-0 bg-slate-800/50 px-2 py-0.5 rounded hidden sm:block max-w-[40%] truncate">
                    {t.definition.length > 40
                      ? t.definition.slice(0, 40) + "..."
                      : t.definition}
                  </code>
                </div>
              </Link>
            ))}
          </div>
        </div>
      ))}

      {grouped.length === 0 && (
        <div className="bg-slate-800/30 border border-slate-700/40 rounded-xl p-5 text-sm text-slate-400">
          {version.types.length === 0
            ? "No types available in this version."
            : "No matching types."}
        </div>
      )}
    </div>
  );
}
