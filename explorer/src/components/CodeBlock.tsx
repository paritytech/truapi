interface CodeBlockProps {
  code: string;
  title?: string;
  language?: string;
}

type TokenType =
  | "comment"
  | "string"
  | "keyword"
  | "number"
  | "function"
  | "operator"
  | "punctuation"
  | "plain";

interface Token {
  type: TokenType;
  value: string;
}

const KEYWORDS = new Set([
  "const",
  "let",
  "var",
  "if",
  "else",
  "return",
  "await",
  "async",
  "for",
  "of",
  "switch",
  "case",
  "break",
  "new",
  "function",
  "try",
  "catch",
  "throw",
  "typeof",
  "instanceof",
  "in",
  "while",
  "do",
  "class",
  "extends",
  "import",
  "from",
  "export",
  "default",
  "yield",
]);

const BUILTINS = new Set([
  "true",
  "false",
  "null",
  "undefined",
  "void",
  "this",
  "super",
]);

function tokenize(code: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < code.length) {
    if (code[i] === "/" && code[i + 1] === "/") {
      const end = code.indexOf("\n", i);
      const slice = end === -1 ? code.slice(i) : code.slice(i, end);
      tokens.push({ type: "comment", value: slice });
      i += slice.length;
      continue;
    }

    if (code[i] === "/" && code[i + 1] === "*") {
      const end = code.indexOf("*/", i + 2);
      const slice = end === -1 ? code.slice(i) : code.slice(i, end + 2);
      tokens.push({ type: "comment", value: slice });
      i += slice.length;
      continue;
    }

    if (code[i] === "`") {
      let j = i + 1;
      while (j < code.length) {
        if (code[j] === "\\") {
          j += 2;
          continue;
        }
        if (code[j] === "`") {
          j++;
          break;
        }
        j++;
      }
      tokens.push({ type: "string", value: code.slice(i, j) });
      i = j;
      continue;
    }

    if (code[i] === '"' || code[i] === "'") {
      const quote = code[i];
      let j = i + 1;
      while (j < code.length) {
        if (code[j] === "\\") {
          j += 2;
          continue;
        }
        if (code[j] === quote) {
          j++;
          break;
        }
        j++;
      }
      tokens.push({ type: "string", value: code.slice(i, j) });
      i = j;
      continue;
    }

    if (
      /[0-9]/.test(code[i]) ||
      (code[i] === "." && /[0-9]/.test(code[i + 1] || ""))
    ) {
      let j = i;
      if (code[j] === "0" && (code[j + 1] === "x" || code[j + 1] === "X")) {
        j += 2;
        while (j < code.length && /[0-9a-fA-F]/.test(code[j])) j++;
      } else {
        while (j < code.length && /[0-9.]/.test(code[j])) j++;
      }
      if (j < code.length && code[j] === "n") j++;
      tokens.push({ type: "number", value: code.slice(i, j) });
      i = j;
      continue;
    }

    if (/[a-zA-Z_$]/.test(code[i])) {
      let j = i;
      while (j < code.length && /[a-zA-Z0-9_$]/.test(code[j])) j++;
      const word = code.slice(i, j);
      if (KEYWORDS.has(word) || BUILTINS.has(word)) {
        tokens.push({ type: "keyword", value: word });
      } else {
        let k = j;
        while (k < code.length && code[k] === " ") k++;
        tokens.push({
          type: code[k] === "(" ? "function" : "plain",
          value: word,
        });
      }
      i = j;
      continue;
    }

    if (/[=<>!&|+\-*/%^~?:]/.test(code[i])) {
      let j = i + 1;
      while (
        j < code.length &&
        /[=<>!&|+\-*/%^~?:]/.test(code[j]) &&
        j - i < 3
      ) {
        j++;
      }
      tokens.push({ type: "operator", value: code.slice(i, j) });
      i = j;
      continue;
    }

    if (/[{}()[\];,.]/.test(code[i])) {
      tokens.push({ type: "punctuation", value: code[i] });
      i++;
      continue;
    }

    tokens.push({ type: "plain", value: code[i] });
    i++;
  }

  return tokens;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function renderTokens(tokens: Token[]): string {
  return tokens
    .map((token) => {
      const escaped = escapeHtml(token.value);
      switch (token.type) {
        case "comment":
          return `<span class="comment">${escaped}</span>`;
        case "string":
          return `<span class="string">${escaped}</span>`;
        case "keyword":
          return `<span class="keyword">${escaped}</span>`;
        case "number":
          return `<span class="number">${escaped}</span>`;
        case "function":
          return `<span class="function">${escaped}</span>`;
        case "operator":
          return `<span class="operator">${escaped}</span>`;
        case "punctuation":
          return `<span class="punctuation">${escaped}</span>`;
        default:
          return escaped;
      }
    })
    .join("");
}

export default function CodeBlock({
  code,
  title,
  language = "typescript",
}: CodeBlockProps) {
  const html = renderTokens(tokenize(code.trim()));

  return (
    <div className="overflow-hidden rounded-lg border border-slate-700/50">
      {title && (
        <div className="flex items-center justify-between border-b border-slate-700/50 bg-slate-800/80 px-4 py-2">
          <span className="text-xs font-medium text-slate-400">{title}</span>
          <span className="font-mono text-[10px] uppercase text-slate-500">
            {language}
          </span>
        </div>
      )}
      <pre className="overflow-x-auto bg-slate-900/80 p-4 text-sm leading-relaxed">
        <code
          className="font-mono text-slate-300"
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </pre>
    </div>
  );
}
