// Generates StaticMCP files (mcp.json + resources/ + tools/) under dist/mcp/
// from the explorer's existing data sources:
//   - src/data/v01/types.ts and v02/types.ts (groups, methods, dataTypes)
//   - docs/rfcs/*.md          (RFC documents)
//   - docs/features/*.md      (feature docs)
//   - v02-changes.md          (changelog)
//
// Spec: https://staticmcp.com/docs/standard
//
// Run via: npx tsx scripts/generate-mcp.ts [outDir]
// Default outDir: dist/mcp

import { readFileSync, readdirSync, mkdirSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as v01 from '../src/data/v01/types.ts';
import * as v02 from '../src/data/v02/types.ts';
import type { DataType, GroupDef, MethodDef } from '../src/data/v01/types.ts';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');
const outDir = resolve(repoRoot, process.argv[2] ?? 'dist/mcp');

// ─── Versions ──────────────────────────────────────────────────────────────

interface VersionData {
  groups: GroupDef[];
  methods: MethodDef[];
  dataTypes: DataType[];
}

interface VersionEntry {
  slug: string;       // 'v01' | 'v02' — used in URIs and filenames
  label: string;      // 'v0.1' | 'v0.2'
  data: VersionData;
}

const versions: VersionEntry[] = [
  { slug: 'v01', label: 'v0.1', data: { groups: v01.groups, methods: v01.methods, dataTypes: v01.dataTypes } },
  { slug: 'v02', label: 'v0.2', data: { groups: v02.groups, methods: v02.methods, dataTypes: v02.dataTypes } },
];

// ─── Helpers ───────────────────────────────────────────────────────────────

// StaticMCP filename encoding: ASCII-fold, lowercase, keep [a-z0-9-_], spaces → _
// Slashes are preserved since URIs contain hierarchical paths.
function encodeUriToPath(uri: string): string {
  // Drop scheme.
  const withoutScheme = uri.replace(/^[a-z][a-z0-9+.-]*:\/\//i, '');
  return withoutScheme
    .split('/')
    .map(encodeSegment)
    .join('/');
}

function encodeSegment(seg: string): string {
  const folded = seg
    .normalize('NFKD')
    .replace(/\p{M}/gu, '');
  let out = '';
  for (const ch of folded) {
    if (/[A-Za-z0-9_-]/.test(ch)) out += ch.toLowerCase();
    else if (ch === ' ' || ch === '.') out += '_';
    // drop everything else
  }
  return out || '_';
}

function writeJson(absPath: string, value: unknown): void {
  mkdirSync(dirname(absPath), { recursive: true });
  writeFileSync(absPath, JSON.stringify(value, null, 2) + '\n');
}

function readMarkdown(absPath: string): { frontmatter: Record<string, string>; body: string; raw: string } {
  const raw = readFileSync(absPath, 'utf8');
  const m = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(raw);
  if (!m) return { frontmatter: {}, body: raw, raw };
  const fmText = m[1];
  const body = m[2];
  const frontmatter: Record<string, string> = {};
  for (const line of fmText.split('\n')) {
    const kv = /^([A-Za-z0-9_-]+)\s*:\s*(.*)$/.exec(line);
    if (!kv) continue;
    let v = kv[2].trim();
    if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) {
      v = v.slice(1, -1);
    }
    frontmatter[kv[1]] = v;
  }
  return { frontmatter, body, raw };
}

// ─── StaticMCP shapes ──────────────────────────────────────────────────────

interface ResourceManifestEntry {
  uri: string;
  name: string;
  description: string;
  mimeType: string;
}

interface ToolManifestEntry {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

interface ResourceFile {
  uri: string;
  mimeType: string;
  text: string;
}

interface ToolFile {
  content: { type: 'text'; text: string }[];
}

const resourceManifest: ResourceManifestEntry[] = [];

function emitResource(entry: ResourceManifestEntry, payload: unknown, mimeType = 'application/json'): void {
  resourceManifest.push(entry);
  const filePath = join(outDir, 'resources', encodeUriToPath(entry.uri) + '.json');
  const text = typeof payload === 'string' ? payload : JSON.stringify(payload, null, 2);
  const file: ResourceFile = { uri: entry.uri, mimeType, text };
  writeJson(filePath, file);
}

function emitTool(toolName: string, argPath: string[], payload: unknown): void {
  const filePath = join(outDir, 'tools', toolName, ...argPath.slice(0, -1), `${argPath[argPath.length - 1]}.json`);
  const text = typeof payload === 'string' ? payload : JSON.stringify(payload, null, 2);
  const file: ToolFile = { content: [{ type: 'text', text }] };
  writeJson(filePath, file);
}

// ─── Reset output ──────────────────────────────────────────────────────────

if (existsSync(outDir)) rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

// ─── Resources: methods, types, groups (per version) ──────────────────────

for (const v of versions) {
  for (const g of v.data.groups) {
    emitResource(
      {
        uri: `truapi://${v.slug}/groups/${g.id}`,
        name: `${v.label} group: ${g.name}`,
        description: g.description,
        mimeType: 'application/json',
      },
      {
        version: v.label,
        id: g.id,
        name: g.name,
        description: g.description,
        methods: g.methods,
      },
    );
  }

  for (const m of v.data.methods) {
    emitResource(
      {
        uri: `truapi://${v.slug}/methods/${m.id}`,
        name: `${v.label} method: ${m.name}`,
        description: m.description,
        mimeType: 'application/json',
      },
      { version: v.label, ...m },
    );
  }

  for (const t of v.data.dataTypes) {
    emitResource(
      {
        uri: `truapi://${v.slug}/types/${t.id}`,
        name: `${v.label} type: ${t.name}`,
        description: t.description,
        mimeType: 'application/json',
      },
      { version: v.label, ...t },
    );
  }
}

// ─── Resources: docs (rfcs, features, changelog) ──────────────────────────

interface DocEntry {
  kind: 'rfcs' | 'features' | 'changelog';
  slug: string;
  title: string;
  description: string;
  body: string;
}

const docs: DocEntry[] = [];

function collectDocs(dirRel: string, kind: 'rfcs' | 'features'): void {
  const absDir = resolve(repoRoot, dirRel);
  if (!existsSync(absDir)) return;
  for (const file of readdirSync(absDir)) {
    if (!file.endsWith('.md')) continue;
    const slug = file.replace(/\.md$/, '');
    if (slug.startsWith('_')) continue; // skip _index.md etc.
    const { frontmatter, body, raw } = readMarkdown(join(absDir, file));
    const title = frontmatter.title || slug;
    const firstParagraph = body
      .split('\n')
      .map(l => l.trim())
      .find(l => l && !l.startsWith('#') && !l.startsWith('|') && !l.startsWith('---')) ?? '';
    docs.push({
      kind,
      slug,
      title,
      description: firstParagraph.slice(0, 240),
      body: raw,
    });
  }
}

collectDocs('docs/rfcs', 'rfcs');
collectDocs('docs/features', 'features');

// Changelog (single file)
{
  const changelogPath = resolve(repoRoot, 'v02-changes.md');
  if (existsSync(changelogPath)) {
    const raw = readFileSync(changelogPath, 'utf8');
    docs.push({
      kind: 'changelog',
      slug: 'v02-changes',
      title: 'v0.2 Protocol Changes',
      description: 'Detailed list of changes from v0.1 to v0.2 with rationale.',
      body: raw,
    });
  }
}

for (const d of docs) {
  emitResource(
    {
      uri: `docs://${d.kind}/${d.slug}`,
      name: d.title,
      description: d.description,
      mimeType: 'text/markdown',
    },
    d.body,
    'text/markdown',
  );
}

// ─── Tools ─────────────────────────────────────────────────────────────────

const toolManifest: ToolManifestEntry[] = [];

// list_versions — single result, single arg ("all") so it fits the spec layout.
toolManifest.push({
  name: 'list_versions',
  description: 'List available TrUAPI protocol versions.',
  inputSchema: {
    type: 'object',
    properties: {
      scope: { type: 'string', enum: ['all'], description: 'Always "all".' },
    },
    required: ['scope'],
  },
});
emitTool('list_versions', ['all'], JSON.stringify(
  versions.map(v => ({ slug: v.slug, label: v.label, status: 'stable' })),
  null,
  2,
));

// list_groups({version})
toolManifest.push({
  name: 'list_groups',
  description: 'List method groups in a TrUAPI protocol version.',
  inputSchema: {
    type: 'object',
    properties: {
      version: { type: 'string', enum: versions.map(v => v.slug), description: 'Version slug, e.g. "v02".' },
    },
    required: ['version'],
  },
});

// list_methods({version})
toolManifest.push({
  name: 'list_methods',
  description: 'List all methods in a TrUAPI protocol version.',
  inputSchema: {
    type: 'object',
    properties: {
      version: { type: 'string', enum: versions.map(v => v.slug) },
    },
    required: ['version'],
  },
});

// list_types({version})
toolManifest.push({
  name: 'list_types',
  description: 'List all data types in a TrUAPI protocol version.',
  inputSchema: {
    type: 'object',
    properties: {
      version: { type: 'string', enum: versions.map(v => v.slug) },
    },
    required: ['version'],
  },
});

for (const v of versions) {
  emitTool('list_groups', [v.slug], JSON.stringify(
    v.data.groups.map(g => ({ id: g.id, name: g.name, description: g.description, methodCount: g.methods.length })),
    null,
    2,
  ));
  emitTool('list_methods', [v.slug], JSON.stringify(
    v.data.methods.map(m => ({
      id: m.id,
      name: m.name,
      groupId: m.groupId,
      pattern: m.pattern,
      description: m.description,
    })),
    null,
    2,
  ));
  emitTool('list_types', [v.slug], JSON.stringify(
    v.data.dataTypes.map(t => ({
      id: t.id,
      name: t.name,
      category: t.category,
      definition: t.definition,
      description: t.description,
    })),
    null,
    2,
  ));
}

// get_method({version, name})
toolManifest.push({
  name: 'get_method',
  description: 'Fetch a method definition by version and name.',
  inputSchema: {
    type: 'object',
    properties: {
      version: { type: 'string', enum: versions.map(v => v.slug) },
      name: { type: 'string', description: 'Method id, e.g. "host_navigate_to".' },
    },
    required: ['version', 'name'],
  },
});

// get_type({version, name})
toolManifest.push({
  name: 'get_type',
  description: 'Fetch a data type definition by version and name.',
  inputSchema: {
    type: 'object',
    properties: {
      version: { type: 'string', enum: versions.map(v => v.slug) },
      name: { type: 'string', description: 'Type id, e.g. "Feature".' },
    },
    required: ['version', 'name'],
  },
});

// get_group({version, id})
toolManifest.push({
  name: 'get_group',
  description: 'Fetch a method group by version and id.',
  inputSchema: {
    type: 'object',
    properties: {
      version: { type: 'string', enum: versions.map(v => v.slug) },
      id: { type: 'string', description: 'Group id, e.g. "truapi-calls".' },
    },
    required: ['version', 'id'],
  },
});

for (const v of versions) {
  for (const m of v.data.methods) {
    emitTool('get_method', [v.slug, encodeSegment(m.id)], JSON.stringify({ version: v.label, ...m }, null, 2));
  }
  for (const t of v.data.dataTypes) {
    emitTool('get_type', [v.slug, encodeSegment(t.id)], JSON.stringify({ version: v.label, ...t }, null, 2));
  }
  for (const g of v.data.groups) {
    const methodsExpanded = g.methods
      .map(id => v.data.methods.find(m => m.id === id))
      .filter((m): m is MethodDef => Boolean(m));
    emitTool('get_group', [v.slug, encodeSegment(g.id)], JSON.stringify(
      { version: v.label, id: g.id, name: g.name, description: g.description, methods: methodsExpanded },
      null,
      2,
    ));
  }
}

// list_docs({kind})
const docKinds = Array.from(new Set(docs.map(d => d.kind)));
toolManifest.push({
  name: 'list_docs',
  description: 'List documentation entries by kind (rfcs, features, changelog).',
  inputSchema: {
    type: 'object',
    properties: {
      kind: { type: 'string', enum: docKinds },
    },
    required: ['kind'],
  },
});
for (const kind of docKinds) {
  emitTool('list_docs', [kind], JSON.stringify(
    docs.filter(d => d.kind === kind).map(d => ({ slug: d.slug, title: d.title, description: d.description })),
    null,
    2,
  ));
}

// get_doc({kind, slug})
toolManifest.push({
  name: 'get_doc',
  description: 'Fetch the full markdown body of a documentation entry.',
  inputSchema: {
    type: 'object',
    properties: {
      kind: { type: 'string', enum: docKinds },
      slug: { type: 'string', description: 'Document slug.' },
    },
    required: ['kind', 'slug'],
  },
});
for (const d of docs) {
  emitTool('get_doc', [d.kind, encodeSegment(d.slug)], d.body);
}

// ─── Manifest ──────────────────────────────────────────────────────────────

const manifest = {
  protocolVersion: '2024-11-05',
  serverInfo: {
    name: 'truapi-explorer',
    version: readJsonField(resolve(repoRoot, 'package.json'), 'version') ?? '0.0.0',
  },
  capabilities: {
    resources: resourceManifest,
    tools: toolManifest,
  },
};

writeJson(join(outDir, 'mcp.json'), manifest);

console.log(`StaticMCP generated at ${outDir}`);
console.log(`  ${resourceManifest.length} resources`);
console.log(`  ${toolManifest.length} tools (with pre-rendered argument combinations)`);

function readJsonField(path: string, field: string): string | undefined {
  try {
    const obj = JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>;
    const v = obj[field];
    return typeof v === 'string' ? v : undefined;
  } catch {
    return undefined;
  }
}
