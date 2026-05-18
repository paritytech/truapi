import type { Monaco } from "@monaco-editor/react";
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import { truapiDts } from "@parity/truapi/playground/codegen/truapi-dts";

// Use the bundled `monaco-editor` package so the playground stays offline-
// deployable. Otherwise `@monaco-editor/react` would fetch Monaco from a CDN
// at runtime.
let loaderConfigured = false;
function configureLoader(): void {
  if (loaderConfigured) return;
  loaderConfigured = true;
  loader.config({ monaco });
}

let monacoConfigured = false;
export function setupMonaco(m: Monaco): void {
  configureLoader();
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

export function ensureLoaderConfigured(): void {
  configureLoader();
}
