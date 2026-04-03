#!/usr/bin/env node
// Transforms rustdoc JSON into TypeScript data files for the API explorer.
// Auto-discovers all version modules (v*) from the rustdoc JSON.
//
// Usage: node scripts/generate-data.mjs

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';

const inputPath = resolve('truapi-spec/target/doc/truapi_spec.json');
const doc = JSON.parse(readFileSync(inputPath, 'utf-8'));
const { index, paths } = doc;

// ── Module discovery ───────────────────────────────────────────────────────

// Discover version modules: items declared directly under the crate root with kind "module"
const modules = new Set();
for (const [id, item] of Object.entries(index)) {
  if (item.inner?.module && paths[id]?.path?.length === 2) {
    modules.add(paths[id].path[1]);
  }
}

// ── Helpers ────────────────────────────────────────────────────────────────

function slugify(name) {
  if (name === 'TrUApiCalls') return 'truapi-calls';
  if (name === 'EntropyDerivation') return 'entropy';
  return name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase();
}

function spacify(name) {
  if (name === 'TrUApiCalls') return 'TrUAPI Calls';
  if (name === 'EntropyDerivation') return 'Entropy';
  return name.replace(/([a-z])([A-Z])/g, '$1 $2');
}

function toCamelCase(snake) {
  return snake.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
}

// ── Type resolution ────────────────────────────────────────────────────────

function resolveType(type) {
  if (!type) return '_void';
  if (type.primitive) return type.primitive === 'bool' ? 'bool' : type.primitive;
  if (type.resolved_path) {
    const name = type.resolved_path.path;
    const args = type.resolved_path.args;

    if (name === 'String') return 'str';
    if (name === 'Vec') {
      const inner = args?.angle_bracketed?.args?.[0]?.type;
      if (inner?.primitive === 'u8') return 'Bytes()';
      return `Vector(${resolveType(inner)})`;
    }
    if (name === 'Option') {
      const inner = args?.angle_bracketed?.args?.[0]?.type;
      return `Option(${resolveType(inner)})`;
    }
    if (name === 'Result') {
      const a = args?.angle_bracketed?.args;
      return `Result(${resolveType(a?.[0]?.type)}, ${resolveType(a?.[1]?.type)})`;
    }
    if (name === 'Box') {
      const inner = args?.angle_bracketed?.args?.[0]?.type;
      if (inner?.dyn_trait) return resolveDynTrait(inner.dyn_trait);
      return resolveType(inner);
    }
    if (name === 'Subscription') {
      const inner = args?.angle_bracketed?.args?.[0]?.type;
      return `Subscription(${resolveType(inner)})`;
    }

    return name;
  }
  if (type.tuple) {
    if (type.tuple.length === 0) return '_void';
    return `Tuple(${type.tuple.map(resolveType).join(', ')})`;
  }
  if (type.array) {
    if (type.array.type?.primitive === 'u8') return `Bytes(${type.array.len})`;
    return `Array(${resolveType(type.array.type)}, ${type.array.len})`;
  }
  if (type.generic) return type.generic;
  if (type.qualified_path) return type.qualified_path.name;
  if (type.dyn_trait) return resolveDynTrait(type.dyn_trait);
  return 'unknown';
}

function resolveDynTrait(dyn_) {
  const t = dyn_.traits?.[0];
  if (!t) return 'unknown';
  const name = t.trait?.path;
  if (name === 'Fn' || name === 'FnMut' || name === 'FnOnce') {
    const inputs = t.trait?.args?.parenthesized?.inputs?.map(resolveType) || [];
    const output = resolveType(t.trait?.args?.parenthesized?.output);
    if (output === '_void') return `${name}(${inputs.join(', ')})`;
    return `${name}(${inputs.join(', ')}) -> ${output}`;
  }
  return name || 'unknown';
}

// ── Doc comment parsing ────────────────────────────────────────────────────

function parseDocs(docs) {
  if (!docs) return { description: '', sections: {} };
  const lines = docs.split('\n');
  let description = '';
  const sections = {};
  let currentSection = null;
  let currentContent = [];

  for (const line of lines) {
    const heading = line.match(/^#\s+(.+)/);
    if (heading) {
      if (currentSection) {
        sections[currentSection] = currentContent.join('\n').trim();
      }
      currentSection = heading[1].trim();
      currentContent = [];
    } else if (currentSection) {
      currentContent.push(line);
    } else {
      description += line + '\n';
    }
  }
  if (currentSection) {
    sections[currentSection] = currentContent.join('\n').trim();
  }

  return { description: description.trim(), sections };
}

function extractCodeBlock(text) {
  if (!text) return '';
  const match = text.match(/```\w*\n([\s\S]*?)```/);
  return match ? match[1].trim() : text.trim();
}

// ── Pattern detection ──────────────────────────────────────────────────────

function detectPattern(method, docs) {
  if (docs.sections['Pattern']) {
    const p = docs.sections['Pattern'].trim().toLowerCase();
    if (p.includes('reverse')) return 'reverse-subscription';
    if (p.includes('subscription')) return 'subscription';
    return 'request-response';
  }

  const output = method.inner.function.sig.output;
  if (isSubscriptionType(output)) return 'subscription';

  if (output?.resolved_path?.path === 'Result') {
    const okType = output.resolved_path.args?.angle_bracketed?.args?.[0]?.type;
    if (isSubscriptionType(okType)) return 'subscription';
  }

  return 'request-response';
}

function isSubscriptionType(type) {
  return type?.resolved_path?.path === 'Subscription';
}

// ── Primitives / combinators ───────────────────────────────────────────────

const basePrimitives = [
  { id: 'str', name: 'str', category: 'Primitives', definition: 'str', description: 'UTF-8 string. SCALE length-prefixed on the wire.' },
  { id: 'bool', name: 'bool', category: 'Primitives', definition: 'bool', description: 'Boolean value (0x00 = false, 0x01 = true).' },
  { id: 'u8', name: 'u8', category: 'Primitives', definition: 'u8', description: 'Unsigned 8-bit integer.' },
  { id: 'u32', name: 'u32', category: 'Primitives', definition: 'u32', description: 'Unsigned 32-bit integer (little-endian SCALE).' },
  { id: 'u64', name: 'u64', category: 'Primitives', definition: 'u64', description: 'Unsigned 64-bit integer (little-endian SCALE).' },
  { id: 'compact', name: 'compact', category: 'Primitives', definition: 'compact', description: 'SCALE compact-encoded unsigned integer (1, 2, or 4 bytes depending on value).' },
  { id: 'Hex', name: 'Hex', category: 'Primitives', definition: 'Hex()', description: 'Hex-encoded arbitrary bytes (SCALE length-prefixed on the wire).' },
  { id: 'Bytes', name: 'Bytes', category: 'Primitives', definition: 'Bytes()', description: 'Arbitrary binary data (SCALE length-prefixed on the wire).' },
  { id: 'BytesN', name: 'BytesN', category: 'Primitives', definition: 'Bytes(N)', description: 'Fixed-length byte array of N bytes.' },
  { id: '_void', name: '_void', category: 'Primitives', definition: '_void', description: 'Empty / unit type. Zero bytes on the wire.' },
  { id: 'Option', name: 'Option', category: 'Combinators', definition: 'Option(T)', description: 'Optional value. SCALE: 0x00 for None, 0x01 ++ encoded(T) for Some.', variants: [{ name: 'None', type: '_void', description: 'No value.' }, { name: 'Some', type: 'T', description: 'A value of type T.' }] },
  { id: 'Nullable', name: 'Nullable', category: 'Combinators', definition: 'Nullable(T)', description: 'Nullable boolean. SCALE: 0x00 = false, 0x01 = true, 0x02 = null.' },
  { id: 'Vector', name: 'Vector', category: 'Combinators', definition: 'Vector(T)', description: 'Variable-length sequence. SCALE: compact length prefix followed by elements.' },
  { id: 'Tuple', name: 'Tuple', category: 'Combinators', definition: 'Tuple(A, B, ...)', description: 'Fixed-length product type. SCALE: elements concatenated in order.' },
  { id: 'Struct', name: 'Struct', category: 'Combinators', definition: 'Struct({ field: T, ... })', description: 'Named-field struct. SCALE: fields encoded in declaration order.' },
  { id: 'Enum', name: 'Enum', category: 'Combinators', definition: 'Enum { Variant(T), ... }', description: 'Tagged union. SCALE: one-byte variant index followed by variant payload.' },
  { id: 'Status', name: 'Status', category: 'Combinators', definition: 'Status(T)', description: 'Subscription status wrapper: the host may push Status events alongside T values.' },
  { id: 'Result', name: 'Result', category: 'Combinators', definition: 'Result(T, E)', description: 'Success or error. SCALE: 0x00 ++ encoded(T) for Ok, 0x01 ++ encoded(E) for Err.', variants: [{ name: 'Ok', type: 'T', description: 'Success value.' }, { name: 'Err', type: 'E', description: 'Error value.' }] },
  { id: 'ErrEnum', name: 'ErrEnum', category: 'Combinators', definition: 'ErrEnum { Variant, ... }', description: 'Error enumeration. Like Enum but used in the error position of Result.' },
];

const u128Prim = { id: 'u128', name: 'u128', category: 'Primitives', definition: 'u128', description: 'Unsigned 128-bit integer (little-endian SCALE).' };

// ── Per-module generation ──────────────────────────────────────────────────

function generateModule(moduleName) {
  function isInModule(id) {
    return paths[id]?.path?.includes(moduleName);
  }

  function rustdocUrl(kind, name) {
    const kindPrefix = kind === 'type_alias' ? 'type' : kind;
    return `rustdoc/truapi_spec/${moduleName}/${kindPrefix}.${name}.html`;
  }

  function getEnumVariantNames(enumId) {
    const item = index[enumId];
    if (!item?.inner?.enum) return undefined;
    return item.inner.enum.variants.map(vid => index[vid]?.name).filter(Boolean);
  }

  // ── Groups + methods from traits ──

  const groups = [];
  const methods = [];

  const traits = Object.entries(index).filter(([id, item]) => {
    if (!item.inner?.trait) return false;
    if (!isInModule(id)) return false;
    if (item.name === 'TrUApi') return false;
    return item.inner.trait.items.length > 0;
  });

  for (const [, traitItem] of traits) {
    const groupId = slugify(traitItem.name);
    const groupName = spacify(traitItem.name);
    const docs = parseDocs(traitItem.docs);
    const methodIds = [];

    for (const mid of traitItem.inner.trait.items) {
      const m = index[mid];
      if (!m?.inner?.function) continue;

      const mDocs = parseDocs(m.docs);
      const pattern = detectPattern(m, mDocs);
      const sig = m.inner.function.sig;
      const inputs = sig.inputs.filter(([name]) => name !== 'self' && name !== 'renderer');
      const output = sig.output;

      let requestType = '_void';
      let responseType = '_void';

      if (inputs.length === 1) {
        requestType = resolveType(inputs[0][1]);
      } else if (inputs.length > 1) {
        requestType = inputs.map(([, t]) => resolveType(t)).join(', ');
      }

      if (pattern === 'subscription') {
        if (output?.resolved_path?.path === 'Subscription') {
          responseType = resolveType(output.resolved_path.args?.angle_bracketed?.args?.[0]?.type);
        } else if (output?.resolved_path?.path === 'Result') {
          const okType = output.resolved_path.args?.angle_bracketed?.args?.[0]?.type;
          if (okType?.resolved_path?.path === 'Subscription') {
            responseType = resolveType(okType.resolved_path.args?.angle_bracketed?.args?.[0]?.type);
          }
        }
      } else {
        responseType = resolveType(output);
      }

      const productFunction = mDocs.sections['Product Function']?.replace(/`/g, '') || `truApi.${toCamelCase(m.name)}(...)`;
      const hostHandler = mDocs.sections['Host Handler']?.replace(/`/g, '') || `container.${toCamelCase('handle_' + m.name)}(handler)`;

      let errorType = undefined;
      let errorVariants = undefined;
      if (output?.resolved_path?.path === 'Result' && pattern === 'request-response') {
        const errType = output.resolved_path.args?.angle_bracketed?.args?.[1]?.type;
        if (errType?.resolved_path) {
          errorType = errType.resolved_path.path;
          errorVariants = getEnumVariantNames(errType.resolved_path.id);
        }
      }

      methods.push({
        id: m.name,
        name: m.name,
        group: groupName,
        groupId,
        pattern,
        description: mDocs.description,
        productFunction,
        hostHandler,
        request: requestType,
        response: responseType,
        requestDescription: mDocs.sections['Request Description'] || undefined,
        responseDescription: mDocs.sections['Response Description'] || undefined,
        errorType,
        errorVariants,
        productExample: extractCodeBlock(mDocs.sections['Product Example']) || `// TODO: add example for ${m.name}`,
        hostExample: extractCodeBlock(mDocs.sections['Host Example']) || `// TODO: add example for ${m.name}`,
        notes: mDocs.sections['Notes'] || undefined,
      });
      methodIds.push(m.name);
    }

    groups.push({
      id: groupId,
      name: groupName,
      description: docs.description,
      methods: methodIds,
    });
  }

  // ── Data types ──

  const dataTypes = [];

  // Check if this module uses u128
  const usesU128 = Object.entries(index).some(([id, item]) => {
    if (!isInModule(id)) return false;
    if (!item.inner?.type_alias) return false;
    const def = resolveType(item.inner.type_alias.type);
    return def === 'u128';
  });

  const prims = [...basePrimitives];
  if (usesU128) {
    prims.splice(5, 0, u128Prim);
  }
  dataTypes.push(...prims);

  for (const [id, item] of Object.entries(index)) {
    if (!isInModule(id)) continue;
    if (item.inner?.trait) continue;
    const path = paths[id]?.path;
    if (!path || path.length !== 3) continue;

    const docs = parseDocs(item.docs);
    const category = docs.sections['Category']?.trim() || 'Common';

    if (item.inner?.struct) {
      const fields = [];
      const kind = item.inner.struct.kind;
      if (kind?.plain) {
        for (const fid of kind.plain.fields) {
          const f = index[fid];
          if (!f) continue;
          fields.push({
            name: toCamelCase(f.name),
            type: resolveType(f.inner.struct_field),
            description: f.docs?.trim() || '',
          });
        }
      }

      dataTypes.push({
        id: item.name,
        name: item.name,
        category,
        source: rustdocUrl('struct', item.name),
        definition: fields.length > 0
          ? `Struct({ ${fields.map(f => `${f.name}: ${f.type}`).join(', ')} })`
          : `Struct({})`,
        description: docs.description,
        fields: fields.length > 0 ? fields : undefined,
      });
    } else if (item.inner?.enum) {
      const variants = [];
      for (const vid of item.inner.enum.variants) {
        const v = index[vid];
        if (!v) continue;
        const kind = v.inner.variant?.kind;
        let type_ = '_void';
        if (kind?.tuple && kind.tuple.length > 0) {
          const types = kind.tuple.map(tid => {
            const t = index[tid];
            return t ? resolveType(t.inner.struct_field) : 'unknown';
          });
          type_ = types.length === 1 ? types[0] : `Tuple(${types.join(', ')})`;
        } else if (kind?.struct) {
          const fields = kind.struct.fields.map(fid => {
            const f = index[fid];
            return f ? `${toCamelCase(f.name)}: ${resolveType(f.inner.struct_field)}` : '';
          }).filter(Boolean);
          type_ = `{ ${fields.join(', ')} }`;
        }
        variants.push({ name: v.name, type: type_, description: v.docs?.trim() || '' });
      }

      const isError = item.name.endsWith('Error') || item.name.endsWith('Err');
      const fmtVariant = v => v.type === '_void' ? v.name : `${v.name}(${v.type})`;
      const definition = isError
        ? `ErrEnum { ${variants.map(fmtVariant).join(', ')} }`
        : `Enum { ${variants.map(fmtVariant).join(', ')} }`;

      dataTypes.push({
        id: item.name,
        name: item.name,
        category,
        source: rustdocUrl('enum', item.name),
        definition,
        description: docs.description,
        variants: variants.length > 0 ? variants : undefined,
      });
    } else if (item.inner?.type_alias) {
      dataTypes.push({
        id: item.name,
        name: item.name,
        category,
        source: rustdocUrl('type_alias', item.name),
        definition: resolveType(item.inner.type_alias.type),
        description: docs.description,
      });
    }
  }

  return { groups, methods, dataTypes };
}

// ── TypeScript output ──────────────────────────────────────────────────────

function generateTypeScript(groups, methods, dataTypes) {
  const lines = [];
  lines.push('// Auto-generated from truapi-spec rustdoc JSON. Do not edit manually.');
  lines.push('// Run: npm run generate');
  lines.push('');
  lines.push(`export interface DataType {`);
  lines.push(`  id: string;`);
  lines.push(`  name: string;`);
  lines.push(`  category: string;`);
  lines.push(`  source?: string;`);
  lines.push(`  definition: string;`);
  lines.push(`  description: string;`);
  lines.push(`  fields?: { name: string; type: string; description: string }[];`);
  lines.push(`  variants?: { name: string; type: string; description: string }[];`);
  lines.push(`}`);
  lines.push('');
  lines.push(`export interface MethodDef {`);
  lines.push(`  id: string;`);
  lines.push(`  name: string;`);
  lines.push(`  group: string;`);
  lines.push(`  groupId: string;`);
  lines.push(`  pattern: 'request-response' | 'subscription' | 'reverse-subscription';`);
  lines.push(`  description: string;`);
  lines.push(`  productFunction: string;`);
  lines.push(`  hostHandler: string;`);
  lines.push(`  request: string;`);
  lines.push(`  response: string;`);
  lines.push(`  requestDescription?: string;`);
  lines.push(`  responseDescription?: string;`);
  lines.push(`  errorType?: string;`);
  lines.push(`  errorVariants?: string[];`);
  lines.push(`  productExample: string;`);
  lines.push(`  hostExample: string;`);
  lines.push(`  notes?: string;`);
  lines.push(`}`);
  lines.push('');
  lines.push(`export interface GroupDef {`);
  lines.push(`  id: string;`);
  lines.push(`  name: string;`);
  lines.push(`  description: string;`);
  lines.push(`  methods: string[];`);
  lines.push(`}`);
  lines.push('');
  lines.push(`export const groups: GroupDef[] = ${JSON.stringify(groups, null, 2)};`);
  lines.push('');
  lines.push(`export const methods: MethodDef[] = ${JSON.stringify(methods, null, 2)};`);
  lines.push('');
  lines.push(`export const dataTypes: DataType[] = ${JSON.stringify(dataTypes, null, 2)};`);
  lines.push('');
  lines.push(`export function getTypeById(id: string): DataType | undefined {`);
  lines.push(`  return dataTypes.find(t => t.id === id);`);
  lines.push(`}`);
  lines.push('');
  lines.push(`export function getMethodById(id: string): MethodDef | undefined {`);
  lines.push(`  return methods.find(m => m.id === id);`);
  lines.push(`}`);
  lines.push('');
  lines.push(`export function getGroupById(id: string): GroupDef | undefined {`);
  lines.push(`  return groups.find(g => g.id === id);`);
  lines.push(`}`);
  lines.push('');
  return lines.join('\n');
}

// ── Main ───────────────────────────────────────────────────────────────────

for (const moduleName of [...modules].sort()) {
  const outDir = `src/data/${moduleName.replace(/_/g, '-')}`;
  const outPath = resolve(`${outDir}/types.ts`);
  mkdirSync(dirname(outPath), { recursive: true });

  const { groups, methods, dataTypes } = generateModule(moduleName);
  writeFileSync(outPath, generateTypeScript(groups, methods, dataTypes), 'utf-8');
  console.log(`[${moduleName}] ${groups.length} groups, ${methods.length} methods, ${dataTypes.length} types -> ${outDir}/types.ts`);
}
