import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useVersion } from '../contexts/VersionContext';

interface TypeLinkProps {
  typeId: string;
  className?: string;
}

export default function TypeLink({ typeId, className = '' }: TypeLinkProps) {
  const navigate = useNavigate();
  const { getTypeById, versionPrefix } = useVersion();
  const dt = getTypeById(typeId);

  if (!dt) {
    return <span className={`font-mono text-slate-300 ${className}`}>{typeId}</span>;
  }

  return (
    <button
      onClick={() => navigate(`${versionPrefix}/type/${typeId}`)}
      className={`font-mono text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors cursor-pointer ${className}`}
    >
      {dt.name}
    </button>
  );
}

// Parse a type string and make type references clickable
export function TypeString({ text, className = '' }: { text: string; className?: string }) {
  const navigate = useNavigate();
  const { dataTypes, versionPrefix } = useVersion();

  // All linkable type IDs, sorted longest-first to avoid partial matches.
  const allTypeIds = useMemo(
    () =>
      dataTypes
        .map(t => t.id)
        .filter(id => id.length > 1)
        .sort((a, b) => b.length - a.length),
    [dataTypes],
  );

  const parts = useMemo(() => {
    const result: { text: string; isType: boolean; typeId: string }[] = [];
    let remaining = text;

    while (remaining.length > 0) {
      let earliestIndex = Infinity;
      let earliestType = '';

      for (const typeId of allTypeIds) {
        const idx = remaining.indexOf(typeId);
        if (idx !== -1 && idx < earliestIndex) {
          // Ensure we match whole words — the character after the match
          // should not be a letter/digit (to avoid matching "bool" inside "boolean")
          const afterIdx = idx + typeId.length;
          const charAfter = remaining[afterIdx];
          const charBefore = idx > 0 ? remaining[idx - 1] : '';
          if (charAfter && /[a-zA-Z0-9_]/.test(charAfter)) continue;
          if (charBefore && /[a-zA-Z0-9_]/.test(charBefore)) continue;

          earliestIndex = idx;
          earliestType = typeId;
        }
      }

      if (earliestType && earliestIndex < Infinity) {
        if (earliestIndex > 0) {
          result.push({ text: remaining.slice(0, earliestIndex), isType: false, typeId: '' });
        }
        result.push({ text: earliestType, isType: true, typeId: earliestType });
        remaining = remaining.slice(earliestIndex + earliestType.length);
      } else {
        result.push({ text: remaining, isType: false, typeId: '' });
        break;
      }
    }

    return result;
  }, [text, allTypeIds]);

  return (
    <span className={`font-mono text-sm ${className}`}>
      {parts.map((part, i) =>
        part.isType ? (
          <button
            key={i}
            onClick={() => navigate(`${versionPrefix}/type/${part.typeId}`)}
            className="text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors cursor-pointer"
          >
            {part.text}
          </button>
        ) : (
          <span key={i} className="text-slate-300">{part.text}</span>
        )
      )}
    </span>
  );
}
