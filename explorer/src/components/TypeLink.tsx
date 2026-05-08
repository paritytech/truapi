import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useVersion } from "../contexts/VersionContext";

export function TypeString({
  text,
  className = "",
}: {
  text: string;
  className?: string;
}) {
  const navigate = useNavigate();
  const { dataTypes, versionPrefix } = useVersion();
  const allTypeIds = useMemo(
    () => dataTypes.map((t) => t.id).sort((a, b) => b.length - a.length),
    [dataTypes],
  );

  const parts = useMemo(() => {
    const result: { text: string; isType: boolean; typeId: string }[] = [];
    let remaining = text;
    while (remaining.length > 0) {
      let earliestIndex = Infinity;
      let earliestType = "";
      for (const typeId of allTypeIds) {
        const idx = remaining.indexOf(typeId);
        if (idx === -1 || idx >= earliestIndex) continue;
        const after = remaining[idx + typeId.length];
        const before = idx > 0 ? remaining[idx - 1] : "";
        if (after && /[a-zA-Z0-9_]/.test(after)) continue;
        if (before && /[a-zA-Z0-9_]/.test(before)) continue;
        earliestIndex = idx;
        earliestType = typeId;
      }
      if (earliestType) {
        if (earliestIndex > 0) {
          result.push({
            text: remaining.slice(0, earliestIndex),
            isType: false,
            typeId: "",
          });
        }
        result.push({ text: earliestType, isType: true, typeId: earliestType });
        remaining = remaining.slice(earliestIndex + earliestType.length);
      } else {
        result.push({ text: remaining, isType: false, typeId: "" });
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
          <span key={i} className="text-slate-300">
            {part.text}
          </span>
        ),
      )}
    </span>
  );
}
