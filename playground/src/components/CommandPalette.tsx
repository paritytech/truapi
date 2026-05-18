import { useEffect, useMemo, useRef, useState } from "react";
import type { ServiceInfo } from "@/src/lib/services";

interface FlatMethod {
  service: string;
  name: string;
  type: "unary" | "subscription";
  description?: string;
  supported: boolean;
}

const KIND_LABEL: Record<string, string> = {
  unary: "Req / Res",
  subscription: "Subscription",
};

function flatten(services: ServiceInfo[]): FlatMethod[] {
  const out: FlatMethod[] = [];
  for (const svc of services) {
    for (const m of svc.methods) {
      out.push({
        service: svc.name,
        name: m.name,
        type: m.type,
        description: m.description,
        supported: !!m.exampleSource && !!m.exampleFunctionName,
      });
    }
  }
  return out;
}

function score(method: FlatMethod, q: string): number {
  if (!q) return 1;
  const query = q.toLowerCase();
  const name = method.name.toLowerCase();
  const service = method.service.toLowerCase();
  const desc = (method.description ?? "").toLowerCase();

  if (name === query) return 1000;
  if (name.startsWith(query)) return 500;
  if (name.includes(query)) return 200;
  if (service.includes(query)) return 100;
  if (desc.includes(query)) return 50;

  let i = 0;
  for (const ch of name) {
    if (ch === query[i]) i++;
    if (i === query.length) return 10;
  }
  return 0;
}

export function CommandPalette({
  services,
  onSelect,
  onClose,
}: {
  services: ServiceInfo[];
  onSelect: (service: string, method: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const all = useMemo(() => flatten(services), [services]);

  const results = useMemo(() => {
    if (!query.trim()) return all;
    return all
      .map((m) => ({ m, s: score(m, query.trim()) }))
      .filter((r) => r.s > 0)
      .sort((a, b) => b.s - a.s)
      .map((r) => r.m);
  }, [all, query]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    setActiveIdx(0);
  }, [query]);

  useEffect(() => {
    const item = listRef.current?.querySelector<HTMLElement>(
      `[data-idx="${activeIdx}"]`,
    );
    item?.scrollIntoView({ block: "nearest" });
  }, [activeIdx]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const hit = results[activeIdx];
      if (hit) onSelect(hit.service, hit.name);
    }
  };

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) onClose();
  };

  return (
    <div
      className="palette-backdrop"
      onMouseDown={handleBackdropClick}
      role="dialog"
      aria-modal="true"
      aria-label="Search methods"
    >
      <div className="palette" onKeyDown={handleKeyDown}>
        <div className="palette__head">
          <span className="palette__icon" aria-hidden>
            ⌕
          </span>
          <input
            ref={inputRef}
            type="text"
            className="palette__input"
            placeholder="Search methods, services, descriptions…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            data-testid="palette-input"
          />
          <button
            type="button"
            className="palette__close"
            onClick={onClose}
            aria-label="Close palette"
          >
            ESC
          </button>
        </div>

        <div className="palette__body" ref={listRef}>
          {results.length === 0 ? (
            <div className="palette__empty">No methods match that query.</div>
          ) : (
            results.map((m, i) => (
              <button
                key={`${m.service}-${m.name}`}
                type="button"
                className="palette__item"
                data-idx={i}
                data-active={i === activeIdx}
                data-supported={m.supported}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => onSelect(m.service, m.name)}
              >
                <div className="palette__item-main">
                  <span className="palette__item-name">{m.name}</span>
                  <span className="palette__item-service">{m.service}</span>
                </div>
                <span className="palette__item-kind" data-kind={m.type}>
                  {m.supported ? KIND_LABEL[m.type] : "n/a"}
                </span>
              </button>
            ))
          )}
        </div>

        <div className="palette__foot">
          <div className="palette__foot-hints">
            <span>
              <kbd className="palette__kbd">↑</kbd>
              <kbd className="palette__kbd">↓</kbd>
              navigate
            </span>
            <span>
              <kbd className="palette__kbd">↵</kbd>
              open
            </span>
            <span>
              <kbd className="palette__kbd">ESC</kbd>
              close
            </span>
          </div>
          <span>
            {results.length} match{results.length === 1 ? "" : "es"}
          </span>
        </div>
      </div>
    </div>
  );
}
