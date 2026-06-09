"use client";

import dynamic from "next/dynamic";
import { useEffect, useState } from "react";
import {
  configureLoader,
  MONACO_THEME_DARK,
  MONACO_THEME_LIGHT,
  setupMonaco,
} from "@/src/lib/monaco-setup";
import type { Monaco } from "@monaco-editor/react";

// Configure the loader to use the bundled monaco-editor before the Editor
// component mounts and calls loader.init(), so it never falls back to the CDN.
const Editor = dynamic(
  async () => {
    const mod = await import("@monaco-editor/react");
    await configureLoader();
    return mod.default;
  },
  { ssr: false },
);

function prefersDark(): boolean {
  if (typeof window === "undefined") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function ExampleEditor({
  source,
  onChange,
  onReady,
  uri,
}: {
  source: string;
  onChange: (next: string) => void;
  onReady?: (monaco: Monaco) => void;
  uri: string;
}) {
  const [isDark, setIsDark] = useState(prefersDark);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  return (
    <div className="example-editor">
      <Editor
        height="320px"
        defaultLanguage="typescript"
        path={uri}
        value={source}
        theme={isDark ? MONACO_THEME_DARK : MONACO_THEME_LIGHT}
        onChange={(v) => onChange(v ?? "")}
        beforeMount={(monaco) => {
          setupMonaco(monaco);
          onReady?.(monaco);
        }}
        onMount={(editor) => {
          const action = editor.getAction("editor.foldAllMarkerRegions");
          if (action) void action.run();
        }}
        options={{
          minimap: { enabled: false },
          fontSize: 13,
          lineNumbers: "off",
          lineDecorationsWidth: 0,
          lineNumbersMinChars: 0,
          glyphMargin: false,
          scrollBeyondLastLine: false,
          tabSize: 2,
          padding: { top: 12, bottom: 12 },
          renderLineHighlight: "line",
          smoothScrolling: true,
          fontFamily:
            'var(--font-mono), ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
          fontLigatures: true,
          guides: { indentation: false },
          overviewRulerLanes: 0,
          hideCursorInOverviewRuler: true,
          overviewRulerBorder: false,
          fixedOverflowWidgets: true,
          scrollbar: {
            verticalScrollbarSize: 8,
            horizontalScrollbarSize: 8,
            useShadows: false,
          },
        }}
      />
    </div>
  );
}
