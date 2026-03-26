#!/usr/bin/env npx tsx

import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { encode } from '@toon-format/toon';
import { groups, methods, dataTypes } from '../src/data/types';

const outdir = resolve(process.argv[2] ?? 'public');
const outfile = resolve(outdir, 'llms.txt');

const data = {
  protocol: {
    name: 'Host API Protocol',
    description: 'Protocol defining methods for sandboxed products to interact with host capabilities',
    methodCount: methods.length,
    groupCount: groups.length,
    typeCount: dataTypes.length,
  },
  groups: groups.map(g => ({
    id: g.id,
    name: g.name,
    description: g.description,
  })),
  methods: methods.map(m => ({
    id: m.id,
    name: m.name,
    groupId: m.groupId,
    pattern: m.pattern,
    productFunction: m.productFunction,
    request: m.request,
    response: m.response,
    description: m.description.slice(0, 100),
  })),
  types: dataTypes.map(t => ({
    id: t.id,
    name: t.name,
    category: t.category,
    definition: t.definition.slice(0, 60),
  })),
};

const toon = encode(data, { delimiter: ',' });
writeFileSync(outfile, toon, 'utf-8');
console.log(`Generated ${outfile}`);
