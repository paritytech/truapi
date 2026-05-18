"use client";

import dynamic from "next/dynamic";
import { setupMonaco } from "@/src/lib/monaco-setup";
import type { Monaco } from "@monaco-editor/react";

const Editor = dynamic(
  async () => (await import("@monaco-editor/react")).default,
  { ssr: false },
);

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
  return (
    <div className="example-editor">
      <Editor
        height="320px"
        defaultLanguage="typescript"
        path={uri}
        value={source}
        onChange={(v) => onChange(v ?? "")}
        beforeMount={(monaco) => {
          setupMonaco(monaco);
          onReady?.(monaco);
        }}
        options={{
          minimap: { enabled: false },
          fontSize: 13,
          scrollBeyondLastLine: false,
          tabSize: 2,
        }}
      />
    </div>
  );
}
