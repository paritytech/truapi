#!/usr/bin/env npx tsx

import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { encode } from '@toon-format/toon';
import { groups, methods, dataTypes } from '../src/data/types';

const outdir = resolve(process.argv[2] ?? 'public');
const outfile = resolve(outdir, 'llms-full.txt');

const data = {
  protocol: {
    name: 'Host API Protocol',
    version: '1.0',
    description: 'Interactive reference for the Host API Protocol — mediates communication between host applications and sandboxed products.',
  },
  groups: groups.map(g => {
    const groupMethods = methods.filter(m => m.groupId === g.id);
    return {
      id: g.id,
      name: g.name,
      description: g.description,
      methodIds: groupMethods.map(m => m.id),
    };
  }),
  methods: methods.map(m => ({
    id: m.id,
    name: m.name,
    groupId: m.groupId,
    pattern: m.pattern,
    productFunction: m.productFunction,
    hostHandler: m.hostHandler,
    request: m.request,
    response: m.response,
    requestDescription: m.requestDescription || '',
    responseDescription: m.responseDescription || '',
    errorType: m.errorType || '',
    errorVariants: m.errorVariants || [],
    description: m.description,
    notes: m.notes || '',
    productExample: m.productExample || '',
    hostExample: m.hostExample || '',
  })),
  types: dataTypes.map(t => ({
    id: t.id,
    name: t.name,
    category: t.category,
    definition: t.definition,
    description: t.description,
    source: t.source || '',
    fields: (t.fields || []).map(f => ({
      name: f.name,
      type: f.type,
      description: f.description,
    })),
    variants: (t.variants || []).map(v => ({
      name: v.name,
      type: v.type,
      description: v.description,
    })),
  })),
};

const toon = encode(data, { delimiter: ',' });
writeFileSync(outfile, toon, 'utf-8');
console.log(`Generated ${outfile}`);
