import type { MethodKind } from "../data/types";

interface PatternBadgeProps {
  kind: MethodKind;
}

const config: Record<
  MethodKind,
  { label: string; bg: string; border: string; text: string }
> = {
  unary: {
    label: "Request / Response",
    bg: "bg-emerald-500/10",
    border: "border-emerald-500/30",
    text: "text-emerald-400",
  },
  subscription: {
    label: "Subscription",
    bg: "bg-amber-500/10",
    border: "border-amber-500/30",
    text: "text-amber-400",
  },
};

/** Pattern pill rendered on method headers and lists. */
export default function PatternBadge({ kind }: PatternBadgeProps) {
  const c = config[kind];
  return (
    <span
      className={`inline-flex items-center shrink-0 whitespace-nowrap px-2.5 py-1 rounded-full text-xs font-medium border ${c.bg} ${c.border} ${c.text}`}
    >
      {c.label}
    </span>
  );
}
