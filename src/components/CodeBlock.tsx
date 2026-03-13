interface CodeBlockProps {
  code: string;
  title?: string;
  language?: string;
}

type TokenType = 'comment' | 'string' | 'keyword' | 'number' | 'function' | 'operator' | 'punctuation' | 'plain';

interface Token {
  type: TokenType;
  value: string;
}

const KEYWORDS = new Set([
  'const', 'let', 'var', 'if', 'else', 'return', 'await', 'async', 'for', 'of',
  'switch', 'case', 'break', 'new', 'function', 'try', 'catch', 'throw', 'typeof',
  'instanceof', 'in', 'while', 'do', 'class', 'extends', 'import', 'from', 'export',
  'default', 'yield',
]);

const BUILTINS = new Set([
  'true', 'false', 'null', 'undefined', 'void', 'this', 'super',
]);

function tokenize(code: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < code.length) {
    // Single-line comment
    if (code[i] === '/' && code[i + 1] === '/') {
      const end = code.indexOf('\n', i);
      const slice = end === -1 ? code.slice(i) : code.slice(i, end);
      tokens.push({ type: 'comment', value: slice });
      i += slice.length;
      continue;
    }

    // Multi-line comment
    if (code[i] === '/' && code[i + 1] === '*') {
      const end = code.indexOf('*/', i + 2);
      const slice = end === -1 ? code.slice(i) : code.slice(i, end + 2);
      tokens.push({ type: 'comment', value: slice });
      i += slice.length;
      continue;
    }

    // Template literal
    if (code[i] === '`') {
      let j = i + 1;
      while (j < code.length) {
        if (code[j] === '\\') { j += 2; continue; }
        if (code[j] === '`') { j++; break; }
        j++;
      }
      tokens.push({ type: 'string', value: code.slice(i, j) });
      i = j;
      continue;
    }

    // Double-quoted string
    if (code[i] === '"') {
      let j = i + 1;
      while (j < code.length) {
        if (code[j] === '\\') { j += 2; continue; }
        if (code[j] === '"') { j++; break; }
        j++;
      }
      tokens.push({ type: 'string', value: code.slice(i, j) });
      i = j;
      continue;
    }

    // Single-quoted string
    if (code[i] === "'") {
      let j = i + 1;
      while (j < code.length) {
        if (code[j] === '\\') { j += 2; continue; }
        if (code[j] === "'") { j++; break; }
        j++;
      }
      tokens.push({ type: 'string', value: code.slice(i, j) });
      i = j;
      continue;
    }

    // Numbers (including hex)
    if (/[0-9]/.test(code[i]) || (code[i] === '.' && /[0-9]/.test(code[i + 1] || ''))) {
      let j = i;
      if (code[j] === '0' && (code[j + 1] === 'x' || code[j + 1] === 'X')) {
        j += 2;
        while (j < code.length && /[0-9a-fA-F]/.test(code[j])) j++;
      } else {
        while (j < code.length && /[0-9.]/.test(code[j])) j++;
      }
      if (j < code.length && code[j] === 'n') j++; // BigInt
      tokens.push({ type: 'number', value: code.slice(i, j) });
      i = j;
      continue;
    }

    // Identifiers and keywords
    if (/[a-zA-Z_$]/.test(code[i])) {
      let j = i;
      while (j < code.length && /[a-zA-Z0-9_$]/.test(code[j])) j++;
      const word = code.slice(i, j);

      if (KEYWORDS.has(word) || BUILTINS.has(word)) {
        tokens.push({ type: 'keyword', value: word });
      } else {
        // Check if followed by ( — it's a function call
        let k = j;
        while (k < code.length && code[k] === ' ') k++;
        if (code[k] === '(') {
          tokens.push({ type: 'function', value: word });
        } else {
          tokens.push({ type: 'plain', value: word });
        }
      }
      i = j;
      continue;
    }

    // Operators and punctuation
    if (/[=<>!&|+\-*/%^~?:]/.test(code[i])) {
      let j = i + 1;
      // Consume multi-char operators
      while (j < code.length && /[=<>!&|+\-*/%^~?:]/.test(code[j]) && j - i < 3) j++;
      tokens.push({ type: 'operator', value: code.slice(i, j) });
      i = j;
      continue;
    }

    if (/[{}()\[\];,.]/.test(code[i])) {
      tokens.push({ type: 'punctuation', value: code[i] });
      i++;
      continue;
    }

    // Whitespace and other
    tokens.push({ type: 'plain', value: code[i] });
    i++;
  }

  return tokens;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function renderTokens(tokens: Token[]): string {
  return tokens.map(t => {
    const escaped = escapeHtml(t.value);
    switch (t.type) {
      case 'comment': return `<span class="comment">${escaped}</span>`;
      case 'string': return `<span class="string">${escaped}</span>`;
      case 'keyword': return `<span class="keyword">${escaped}</span>`;
      case 'number': return `<span class="number">${escaped}</span>`;
      case 'function': return `<span class="function">${escaped}</span>`;
      case 'operator': return `<span class="operator">${escaped}</span>`;
      case 'punctuation': return `<span class="punctuation">${escaped}</span>`;
      default: return escaped;
    }
  }).join('');
}

export default function CodeBlock({ code, title, language = 'typescript' }: CodeBlockProps) {
  const tokens = tokenize(code.trim());
  const html = renderTokens(tokens);

  return (
    <div className="rounded-lg border border-slate-700/50 overflow-hidden">
      {title && (
        <div className="bg-slate-800/80 border-b border-slate-700/50 px-4 py-2 flex items-center justify-between">
          <span className="text-xs text-slate-400 font-medium">{title}</span>
          <span className="text-[10px] font-mono text-slate-500 uppercase">{language}</span>
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
