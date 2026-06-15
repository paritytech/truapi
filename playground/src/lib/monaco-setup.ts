import type { Monaco } from "@monaco-editor/react";
import { loader } from "@monaco-editor/react";
import type { Environment } from "monaco-editor";
import { truapiDts } from "@parity/truapi/playground/codegen/truapi-dts";
import { rxjsFiles } from "./codegen/rxjs-dts";

export const MONACO_THEME_LIGHT = "truapi-light";
export const MONACO_THEME_DARK = "truapi-dark";

// Bundle the web workers from the local `monaco-editor` package. Without this,
// the editor falls back to the AMD loader's `require.toUrl`, which the ESM
// build does not provide (TypeError: reading 'toUrl'). webpack emits each
// `new Worker(new URL(...))` as its own chunk served from our own origin.
function monacoWorker(label: string): Worker {
  if (label === "typescript" || label === "javascript") {
    return new Worker(
      new URL(
        "monaco-editor/esm/vs/language/typescript/ts.worker.js",
        import.meta.url,
      ),
    );
  }
  return new Worker(
    new URL("monaco-editor/esm/vs/editor/editor.worker.js", import.meta.url),
  );
}

let loaderConfigured = false;
/**
 * Point `@monaco-editor/react`'s loader at the bundled `monaco-editor` package
 * instead of its default jsdelivr CDN. Must run before the Editor mounts and
 * calls `loader.init()`, otherwise the loader falls back to the CDN.
 */
export async function configureLoader(): Promise<void> {
  if (loaderConfigured) return;
  loaderConfigured = true;
  (
    globalThis as typeof globalThis & { MonacoEnvironment?: Environment }
  ).MonacoEnvironment = { getWorker: (_id, label) => monacoWorker(label) };
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
      `   * for every \`Initialized\` event. The returned Observable completes`,
      `   * on \`Stop\` and errors on \`OperationError\` and \`OperationInaccessible\`.`,
      `   */`,
      `  function withChainHeadFollow(opts: {`,
      `    genesisHash: \`0x\${string}\`;`,
      `    withRuntime?: boolean;`,
      `  }): import("rxjs").Observable<ChainHeadCtx>;`,
      `  /** Resolve a DotNS username to the owning raw AccountId32 hex string. Defaults to truapi.account.getUserId(). */`,
      `  function accountIdForDotNsUsername(username?: string): Promise<import("neverthrow").Result<\`0x\${string}\`, Error>>;`,
      `  /**`,
      `   * Assert a condition, throwing when it does not hold. Examples signal`,
      `   * failure explicitly with \`assert(...)\`; the diagnosis marks an example`,
      `   * failed when it throws and passed when it runs to completion.`,
      `   */`,
      `  function assert(condition: unknown, ...message: unknown[]): asserts condition;`,
      `  interface Console {`,
      `    log(...args: unknown[]): void;`,
      `    warn(...args: unknown[]): void;`,
      `    error(...args: unknown[]): void;`,
      `  }`,
      `  const console: Console;`,
      `  // Monaco bundles a trimmed TypeScript lib that omits WebCrypto and the`,
      `  // newer Uint8Array hex helpers the examples use; declare them so the`,
      `  // editor matches the repo's tsc.`,
      `  const crypto: {`,
      `    getRandomValues<T extends ArrayBufferView>(array: T): T;`,
      `    randomUUID(): string;`,
      `  };`,
      `  interface Uint8Array {`,
      `    toHex(): string;`,
      `  }`,
      `  interface Uint8ArrayConstructor {`,
      `    fromHex(hex: string): Uint8Array;`,
      `  }`,
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
