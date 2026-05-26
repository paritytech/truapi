import { useMemo } from "react";
import { Link } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import type { DataType } from "../data/types";

interface MarkdownTextProps {
  text: string;
  versionId: string;
  types: DataType[];
  className?: string;
}

/**
 * Renders a rustdoc description string as Markdown:
 * - inline code spans (`` `Foo` ``) get monospace styling
 * - intra-doc links (`[Foo](crate::path::Foo)`, `Self::`, etc.) resolve to
 *   the explorer's `/v/.../type/<id>` page when the trailing identifier
 *   matches a known DataType; otherwise the link is dropped and the inner
 *   text is shown plain
 * - real URLs open in a new tab
 */
export function MarkdownText({
  text,
  versionId,
  types,
  className,
}: MarkdownTextProps) {
  const nameToId = useMemo(() => {
    const map: Record<string, string> = {};
    for (const t of types) map[t.name] = t.id;
    return map;
  }, [types]);

  return (
    <div className={`markdown ${className ?? ""}`}>
      <ReactMarkdown
        components={{
          a: ({ href, children }) => {
            if (!href) return <>{children}</>;
            if (
              /^(crate|self|super|Self)::/.test(href) ||
              !/^(https?:)?\/\//.test(href)
            ) {
              const tail = href.split("::").pop() ?? href;
              const cleaned = tail.replace(/[()].*$/, "");
              const id = nameToId[cleaned];
              if (id) {
                return (
                  <Link
                    to={`/v/${versionId}/type/${id}`}
                    className="text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors"
                  >
                    {children}
                  </Link>
                );
              }
              return <>{children}</>;
            }
            return (
              <a
                href={href}
                target="_blank"
                rel="noreferrer"
                className="text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors"
              >
                {children}
              </a>
            );
          },
          code: ({ children }) => (
            <code className="px-1 py-0.5 rounded bg-slate-800/60 text-slate-200 text-[0.9em] font-mono">
              {children}
            </code>
          ),
          p: ({ children }) => (
            <p className="leading-relaxed last:mb-0 mb-2">{children}</p>
          ),
          ul: ({ children }) => (
            <ul className="list-disc pl-5 mb-2 space-y-0.5">{children}</ul>
          ),
          ol: ({ children }) => (
            <ol className="list-decimal pl-5 mb-2 space-y-0.5">{children}</ol>
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
