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
  const sortedIds = useMemo(
    () =>
      types
        .map((t) => t.id)
        .filter((id) => id.length > 1)
        .sort((a, b) => b.length - a.length),
    [types],
  );
  const idToName = useMemo(() => {
    const map: Record<string, string> = {};
    for (const t of types) map[t.id] = t.name;
    return map;
  }, [types]);

  const parts = useMemo(() => {
    const result: { text: string; typeId: string | null }[] = [];
    let remaining = text;

    while (remaining.length > 0) {
      let earliestIndex = Infinity;
      let earliestType = "";

      for (const id of sortedIds) {
        const idx = remaining.indexOf(id);
        if (idx === -1 || idx >= earliestIndex) continue;
        const before = idx > 0 ? remaining[idx - 1] : "";
        const after = remaining[idx + id.length];
        if (before && /[a-zA-Z0-9_]/.test(before)) continue;
        if (after && /[a-zA-Z0-9_]/.test(after)) continue;
        earliestIndex = idx;
        earliestType = id;
      }

      if (earliestType && earliestIndex < Infinity) {
        if (earliestIndex > 0) {
          result.push({
            text: remaining.slice(0, earliestIndex),
            typeId: null,
          });
        }
        result.push({
          text: idToName[earliestType] ?? earliestType,
          typeId: earliestType,
        });
        remaining = remaining.slice(earliestIndex + earliestType.length);
      } else {
        result.push({ text: remaining, typeId: null });
        break;
      }
    }

    return result;
  }, [text, sortedIds, idToName]);

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
