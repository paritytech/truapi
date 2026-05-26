type TokenType =
  | "comment"
  | "string"
  | "keyword"
  | "number"
  | "function"
  | "operator"
  | "punctuation"
  | "type"
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
  "interface",
  "type",
  "enum",
  "as",
  "is",
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
    const ch = code[i];
    const next = code[i + 1];

    if (ch === "/" && next === "/") {
      const end = code.indexOf("\n", i);
      const slice = end === -1 ? code.slice(i) : code.slice(i, end);
      tokens.push({ type: "comment", value: slice });
      i += slice.length;
      continue;
    }

    if (ch === "/" && next === "*") {
      const end = code.indexOf("*/", i + 2);
      const slice = end === -1 ? code.slice(i) : code.slice(i, end + 2);
      tokens.push({ type: "comment", value: slice });
      i += slice.length;
      continue;
    }

    if (ch === "`" || ch === '"' || ch === "'") {
      const quote = ch;
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

    if (/[0-9]/.test(ch) || (ch === "." && /[0-9]/.test(next ?? ""))) {
      let j = i;
      if (ch === "0" && (next === "x" || next === "X")) {
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

    if (/[a-zA-Z_$]/.test(ch)) {
      let j = i;
      while (j < code.length && /[a-zA-Z0-9_$]/.test(code[j])) j++;
      const word = code.slice(i, j);

      if (KEYWORDS.has(word) || BUILTINS.has(word)) {
        tokens.push({ type: "keyword", value: word });
      } else {
        let k = j;
        while (k < code.length && code[k] === " ") k++;
        if (code[k] === "(") {
          tokens.push({ type: "function", value: word });
        } else if (/^[A-Z]/.test(word)) {
          tokens.push({ type: "type", value: word });
        } else {
          tokens.push({ type: "plain", value: word });
        }
      }
      i = j;
      continue;
    }

    if (/[=<>!&|+\-*/%^~?:]/.test(ch)) {
      let j = i + 1;
      while (j < code.length && /[=<>!&|+\-*/%^~?:]/.test(code[j]) && j - i < 3)
        j++;
      tokens.push({ type: "operator", value: code.slice(i, j) });
      i = j;
      continue;
    }

    if (/[{}()\[\];,.]/.test(ch)) {
      tokens.push({ type: "punctuation", value: ch });
      i++;
      continue;
    }

    tokens.push({ type: "plain", value: ch });
    i++;
  }

  return tokens;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function renderTokens(tokens: Token[]): string {
  return tokens
    .map((t) => {
      const escaped = escapeHtml(t.value);
      if (t.type === "plain") return escaped;
      return `<span class="${t.type}">${escaped}</span>`;
    })
    .join("");
}

interface CodeBlockProps {
  code: string;
  title?: string;
  language?: string;
}

/** Minimal TS-flavoured code block with regex tokenizer styling. */
export default function CodeBlock({
  code,
  title,
  language = "typescript",
}: CodeBlockProps) {
  const html = renderTokens(tokenize(code.trim()));
  return (
    <div className="rounded-lg border border-slate-700/50 overflow-hidden">
      {title && (
        <div className="bg-slate-800/80 border-b border-slate-700/50 px-4 py-2 flex items-center justify-between">
          <span className="text-xs text-slate-400 font-medium">{title}</span>
          <span className="text-[10px] font-mono text-slate-500 uppercase">
            {language}
          </span>
        </div>
      )}
      <pre className="bg-slate-900/80 p-4 overflow-x-auto text-sm leading-relaxed">
        <code
          className="font-mono text-slate-300"
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </pre>
    </div>
  );
}
