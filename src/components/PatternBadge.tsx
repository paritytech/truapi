import { ArrowLeftRight, Radio, RotateCcw } from 'lucide-react';

interface PatternBadgeProps {
  pattern: 'request-response' | 'subscription' | 'reverse-subscription';
}

const config = {
  'request-response': {
    label: 'Request / Response',
    icon: ArrowLeftRight,
    bg: 'bg-emerald-500/10',
    border: 'border-emerald-500/30',
    text: 'text-emerald-400',
  },
  'subscription': {
    label: 'Subscription',
    icon: Radio,
    bg: 'bg-amber-500/10',
    border: 'border-amber-500/30',
    text: 'text-amber-400',
  },
  'reverse-subscription': {
    label: 'Reverse Subscription',
    icon: RotateCcw,
    bg: 'bg-purple-500/10',
    border: 'border-purple-500/30',
    text: 'text-purple-400',
  },
};

export default function PatternBadge({ pattern }: PatternBadgeProps) {
  const c = config[pattern];
  const Icon = c.icon;

  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border ${c.bg} ${c.border} ${c.text}`}
    >
      <Icon size={12} />
      {c.label}
    </span>
  );
}
