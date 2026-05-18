import type { Monaco } from "@monaco-editor/react";
import { loader } from "@monaco-editor/react";
import { truapiDts } from "@parity/truapi/playground/codegen/truapi-dts";

let loaderConfigured = false;
async function configureLoader(): Promise<void> {
  if (loaderConfigured) return;
  loaderConfigured = true;
  // Lazy-import monaco-editor on the client only. Importing it eagerly at
  // module scope crashes Next's SSR prerender (`window is not defined`).
  const monaco = await import("monaco-editor");
  loader.config({ monaco });
}

let monacoConfigured = false;
export function setupMonaco(m: Monaco): void {
  void configureLoader();
  if (monacoConfigured) return;
  monacoConfigured = true;

  const ts = m.languages.typescript;
  ts.typescriptDefaults.setCompilerOptions({
    target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.NodeJs,
    strict: true,
    noEmitOnError: false,
    allowNonTsExtensions: true,
    esModuleInterop: true,
    lib: ["esnext", "dom"],
  });

  ts.typescriptDefaults.addExtraLib(
    `declare module "@parity/truapi" {\n${truapiDts}\n}\n`,
    "file:///node_modules/@parity/truapi/index.d.ts",
  );

  ts.typescriptDefaults.setDiagnosticsOptions({
    noSemanticValidation: false,
    noSyntaxValidation: false,
  });
}
