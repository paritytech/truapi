import { Highlight, themes } from "prism-react-renderer";

/** Minimal TS-flavoured code block, syntax-highlighted by prism-react-renderer. */
export default function CodeBlock({ code }: { code: string }) {
  return (
    <Highlight
      code={code.trim()}
      language="typescript"
      theme={themes.vsDark}
    >
      {({ className, style, tokens, getLineProps, getTokenProps }) => (
        <pre
          className={`${className} rounded-lg border border-slate-700/50 p-4 overflow-x-auto text-sm leading-relaxed font-mono`}
          style={{ ...style, background: "rgb(15 23 42 / 0.8)" }}
        >
          {tokens.map((line, i) => (
            <div key={i} {...getLineProps({ line })}>
              {line.map((token, key) => (
                <span key={key} {...getTokenProps({ token })} />
              ))}
            </div>
          ))}
        </pre>
      )}
    </Highlight>
  );
}
