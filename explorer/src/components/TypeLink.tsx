import { Fragment, useMemo } from "react";
import { Link } from "react-router-dom";
import type { DataType } from "../data/types";

interface TypeStringProps {
  text: string;
  versionId: string;
  types: DataType[];
  className?: string;
}

/**
 * Render a string with known type names replaced by links to their detail page.
 * Matches are whole-word and prefer the longest type name when several match
 * at the same position.
 */
export function TypeString({
  text,
  versionId,
  types,
  className = "",
}: TypeStringProps) {
  const sortedNames = useMemo(
    () =>
      types
        .map((t) => t.name)
        .filter((name) => name.length > 1)
        .sort((a, b) => b.length - a.length),
    [types],
  );
  const nameToId = useMemo(() => {
    const map: Record<string, string> = {};
    for (const t of types) map[t.name] = t.id;
    return map;
  }, [types]);

  const parts = useMemo(() => {
    const result: { text: string; typeId: string | null }[] = [];
    let remaining = text;

    while (remaining.length > 0) {
      let earliestIndex = Infinity;
      let earliestName = "";

      for (const name of sortedNames) {
        const idx = remaining.indexOf(name);
        if (idx === -1 || idx >= earliestIndex) continue;
        const before = idx > 0 ? remaining[idx - 1] : "";
        const after = remaining[idx + name.length];
        if (before && /[a-zA-Z0-9_]/.test(before)) continue;
        if (after && /[a-zA-Z0-9_]/.test(after)) continue;
        earliestIndex = idx;
        earliestName = name;
      }

      if (earliestName && earliestIndex < Infinity) {
        if (earliestIndex > 0) {
          result.push({
            text: remaining.slice(0, earliestIndex),
            typeId: null,
          });
        }
        result.push({
          text: earliestName,
          typeId: nameToId[earliestName] ?? null,
        });
        remaining = remaining.slice(earliestIndex + earliestName.length);
      } else {
        result.push({ text: remaining, typeId: null });
        break;
      }
    }

    return result;
  }, [text, sortedNames, nameToId]);

  return (
    <span className={`font-mono text-sm ${className}`}>
      {parts.map((part, i) =>
        part.typeId ? (
          <Link
            key={i}
            to={`/v/${versionId}/type/${part.typeId}`}
            className="text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors"
          >
            {part.text}
          </Link>
        ) : (
          <Fragment key={i}>
            <span className="text-slate-300">{part.text}</span>
          </Fragment>
        ),
      )}
    </span>
  );
}
