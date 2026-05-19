import type { Monaco } from "@monaco-editor/react";
import { loader } from "@monaco-editor/react";
import { truapiDts } from "@parity/truapi/playground/codegen/truapi-dts";
import { rxjsFiles } from "./codegen/rxjs-dts";

export const MONACO_THEME_LIGHT = "truapi-light";
export const MONACO_THEME_DARK = "truapi-dark";

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
    moduleDetection: 3 as never,
  });

  ts.typescriptDefaults.addExtraLib(
    `declare module "@parity/truapi" {\n${truapiDts}\n}\n`,
    "file:///node_modules/@parity/truapi/index.d.ts",
  );

  for (const file of rxjsFiles) {
    ts.typescriptDefaults.addExtraLib(file.content, file.path);
  }

  ts.typescriptDefaults.addExtraLib(
    [
      `declare global {`,
      `  const truapi: import("@parity/truapi").Client;`,
      `  /**`,
      `   * Per-block context emitted by {@link withChainHeadFollow}. \`hash\` is`,
      `   * the first finalized block hash reported on the underlying`,
      `   * \`chainHead_follow\` subscription, and \`followSubscriptionId\` is the`,
      `   * id that subsequent chain-head requests must reference.`,
      `   */`,
      `  type ChainHeadCtx = {`,
      `    genesisHash: \`0x\${string}\`;`,
      `    followSubscriptionId: string;`,
      `    hash: \`0x\${string}\`;`,
      `  };`,
      `  /**`,
      `   * Start a \`chainHead_follow\` subscription and emit a {@link ChainHeadCtx}`,
      `   * for every \`Initialized\` event. The returned Observable errors on`,
      `   * \`Stop\`, \`OperationError\`, and \`OperationInaccessible\``,
      `   */`,
      `  function withChainHeadFollow(opts: {`,
      `    genesisHash: \`0x\${string}\`;`,
      `    withRuntime?: boolean;`,
      `  }): import("rxjs").Observable<ChainHeadCtx>;`,
      `  interface Console {`,
      `    log(...args: unknown[]): void;`,
      `    warn(...args: unknown[]): void;`,
      `    error(...args: unknown[]): void;`,
      `  }`,
      `  const console: Console;`,
      `}`,
      `export {};`,
      ``,
    ].join("\n"),
    "file:///playground/__ambient.d.ts",
  );

  ts.typescriptDefaults.setDiagnosticsOptions({
    noSemanticValidation: false,
    noSyntaxValidation: false,
  });

  m.editor.defineTheme(MONACO_THEME_LIGHT, {
    base: "vs",
    inherit: true,
    rules: [
      { token: "comment", foreground: "8F877F", fontStyle: "italic" },
      { token: "keyword", foreground: "E6007A" },
      { token: "string", foreground: "1F6B4A" },
      { token: "number", foreground: "B7590F" },
      { token: "type", foreground: "4E4741" },
      { token: "type.identifier", foreground: "4E4741" },
      { token: "identifier", foreground: "17120F" },
    ],
    colors: {
      "editor.background": "#FBF8F1",
      "editor.foreground": "#17120F",
      "editorLineNumber.foreground": "#B9B0A3",
      "editorLineNumber.activeForeground": "#4E4741",
      "editor.lineHighlightBackground": "#F4EFE4",
      "editor.lineHighlightBorder": "#00000000",
      "editor.selectionBackground": "#FCE4EF",
      "editor.inactiveSelectionBackground": "#F4EFE4",
      "editorCursor.foreground": "#E6007A",
      "editorIndentGuide.background": "#E1D9C7",
      "editorIndentGuide.activeBackground": "#CFC5AE",
      "editorWhitespace.foreground": "#E1D9C7",
      "editorGutter.background": "#FBF8F1",
      "editorWidget.background": "#FBF8F1",
      "editorWidget.border": "#CFC5AE",
      "editorSuggestWidget.background": "#FBF8F1",
      "editorSuggestWidget.border": "#CFC5AE",
      "editorSuggestWidget.selectedBackground": "#F4EFE4",
      "editorHoverWidget.background": "#FBF8F1",
      "editorHoverWidget.border": "#CFC5AE",
      "scrollbarSlider.background": "#CFC5AE66",
      "scrollbarSlider.hoverBackground": "#CFC5AE99",
      "scrollbarSlider.activeBackground": "#CFC5AEcc",
    },
  });

  m.editor.defineTheme(MONACO_THEME_DARK, {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "877F77", fontStyle: "italic" },
      { token: "keyword", foreground: "FF4EA8" },
      { token: "string", foreground: "3FAE7F" },
      { token: "number", foreground: "E08648" },
      { token: "type", foreground: "B9B1A4" },
      { token: "type.identifier", foreground: "B9B1A4" },
      { token: "identifier", foreground: "EFE9DD" },
    ],
    colors: {
      "editor.background": "#1F1C19",
      "editor.foreground": "#EFE9DD",
      "editorLineNumber.foreground": "#5C554F",
      "editorLineNumber.activeForeground": "#B9B1A4",
      "editor.lineHighlightBackground": "#1A1816",
      "editor.lineHighlightBorder": "#00000000",
      "editor.selectionBackground": "#FF4EA840",
      "editor.inactiveSelectionBackground": "#2A2521",
      "editorCursor.foreground": "#FF4EA8",
      "editorIndentGuide.background": "#2A2521",
      "editorIndentGuide.activeBackground": "#3A332C",
      "editorWhitespace.foreground": "#2A2521",
      "editorGutter.background": "#1F1C19",
      "editorWidget.background": "#1F1C19",
      "editorWidget.border": "#3A332C",
      "editorSuggestWidget.background": "#1F1C19",
      "editorSuggestWidget.border": "#3A332C",
      "editorSuggestWidget.selectedBackground": "#2A2521",
      "editorHoverWidget.background": "#1F1C19",
      "editorHoverWidget.border": "#3A332C",
      "scrollbarSlider.background": "#3A332C66",
      "scrollbarSlider.hoverBackground": "#3A332C99",
      "scrollbarSlider.activeBackground": "#3A332Ccc",
    },
  });
}
