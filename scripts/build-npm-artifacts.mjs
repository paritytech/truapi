#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = resolve(root, "packages/truapi");
const versionDir = resolve(outDir, "v0.2");
const packageName = "@useragent-kit/truapi";
const protocolVersion = "0.2";
const publishedRevision = "a7fa645";

const data = await import("../src/data/v02/types.ts");
const rustSpec = readFileSync(resolve(root, "truapi-spec/src/v02/mod.rs"), "utf8");
const revision = process.env.TRUAPI_ARTIFACT_REVISION ?? publishedRevision;

const methodByName = new Map(data.methods.map((method) => [method.name, method]));
const orderedMethodNames = [...rustSpec.matchAll(/^\s+fn\s+([a-z0-9_]+)\s*\(/gm)].map(
  (match) => match[1],
);

if (orderedMethodNames.length !== data.methods.length) {
  throw new Error(
    `Expected ${data.methods.length} v0.2 methods in Rust spec, found ${orderedMethodNames.length}`,
  );
}

const methods = orderedMethodNames.map((name, index) => {
  const method = methodByName.get(name);
  if (!method) {
    throw new Error(`Rust spec method ${name} is missing from v0.2 TypeScript registry`);
  }
  return {
    name,
    tag: index,
    kind:
      method.pattern === "request-response"
        ? "request"
        : method.pattern === "reverse-subscription"
          ? "reverse-subscription"
          : "subscription",
    group: method.groupId,
    request: method.request,
    response: method.response,
    errorType: method.errorType ?? null,
  };
});

const manifest = {
  schemaVersion: 1,
  protocol: {
    name: "TrUAPI",
    version: protocolVersion,
    source: {
      repo: "https://github.com/paritytech/truapi",
      path: "truapi-spec/src/v02/mod.rs",
      revision,
    },
    transport: "message-port",
    wireFormat: "scale-host-api",
  },
  methods,
  groups: data.groups.map((group) => ({
    id: group.id,
    name: group.name,
    description: group.description,
    methods: group.methods,
  })),
  dataTypes: data.dataTypes.map((dataType) => ({
    id: dataType.id,
    name: dataType.name,
    category: dataType.category,
    source: dataType.source ?? null,
    definition: dataType.definition,
    description: dataType.description,
    fields: dataType.fields ?? [],
    variants: dataType.variants ?? [],
  })),
  deprecatedAliases: {
    host_get_non_product_accounts: "host_get_legacy_accounts",
    host_create_transaction_with_non_product_account:
      "host_create_transaction_with_legacy_account",
  },
};

function write(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

function jsModuleFor(manifest) {
  return `export const manifest = ${JSON.stringify(manifest, null, 2)};\n` +
    `export const methods = manifest.methods;\n` +
    `export const groups = manifest.groups;\n` +
    `export const dataTypes = manifest.dataTypes;\n` +
    `export const protocolVersion = manifest.protocol.version;\n` +
    `export default manifest;\n`;
}

const methodNames = methods.map((method) => JSON.stringify(method.name)).join(" | ");
const dataTypeNames = manifest.dataTypes.map((dataType) => JSON.stringify(dataType.id)).join(" | ");

const dts = `export type TrUApiMethodKind = "request" | "subscription" | "reverse-subscription";\n` +
  `export type TrUApiMethodName = ${methodNames};\n` +
  `export type TrUApiDataTypeName = ${dataTypeNames};\n\n` +
  `export interface TrUApiMethodArtifact {\n` +
  `  readonly name: TrUApiMethodName;\n` +
  `  readonly tag: number;\n` +
  `  readonly kind: TrUApiMethodKind;\n` +
  `  readonly group: string;\n` +
  `  readonly request: string;\n` +
  `  readonly response: string;\n` +
  `  readonly errorType: string | null;\n` +
  `}\n\n` +
  `export interface TrUApiGroupArtifact {\n` +
  `  readonly id: string;\n` +
  `  readonly name: string;\n` +
  `  readonly description: string;\n` +
  `  readonly methods: readonly TrUApiMethodName[];\n` +
  `}\n\n` +
  `export interface TrUApiDataTypeArtifact {\n` +
  `  readonly id: TrUApiDataTypeName;\n` +
  `  readonly name: string;\n` +
  `  readonly category: string;\n` +
  `  readonly source: string | null;\n` +
  `  readonly definition: string;\n` +
  `  readonly description: string;\n` +
  `  readonly fields: readonly { readonly name: string; readonly type: string; readonly description: string }[];\n` +
  `  readonly variants: readonly { readonly name: string; readonly type: string; readonly description: string }[];\n` +
  `}\n\n` +
  `export interface TrUApiManifest {\n` +
  `  readonly schemaVersion: 1;\n` +
  `  readonly protocol: {\n` +
  `    readonly name: "TrUAPI";\n` +
  `    readonly version: "0.2";\n` +
  `    readonly source: { readonly repo: string; readonly path: string; readonly revision: string };\n` +
  `    readonly transport: "message-port";\n` +
  `    readonly wireFormat: "scale-host-api";\n` +
  `  };\n` +
  `  readonly methods: readonly TrUApiMethodArtifact[];\n` +
  `  readonly groups: readonly TrUApiGroupArtifact[];\n` +
  `  readonly dataTypes: readonly TrUApiDataTypeArtifact[];\n` +
  `  readonly deprecatedAliases: Readonly<Record<string, TrUApiMethodName>>;\n` +
  `}\n\n` +
  `export declare const manifest: TrUApiManifest;\n` +
  `export declare const methods: readonly TrUApiMethodArtifact[];\n` +
  `export declare const groups: readonly TrUApiGroupArtifact[];\n` +
  `export declare const dataTypes: readonly TrUApiDataTypeArtifact[];\n` +
  `export declare const protocolVersion: "0.2";\n` +
  `export default manifest;\n`;

const packageJson = {
  name: packageName,
  version: "0.2.0",
  description: "Versioned TrUAPI protocol artifacts: TypeScript types, method registry, and manifest.",
  type: "module",
  exports: {
    ".": {
      types: "./index.d.ts",
      import: "./index.js",
    },
    "./v0.2": {
      types: "./v0.2/index.d.ts",
      import: "./v0.2/index.js",
    },
    "./v0.2/manifest.json": "./v0.2/manifest.json",
  },
  files: ["LICENSE", "README.md", "index.d.ts", "index.js", "v0.2"],
  sideEffects: false,
  publishConfig: {
    access: "public",
  },
  repository: {
    type: "git",
    url: "https://github.com/paritytech/truapi.git",
    directory: "packages/truapi",
  },
  license: "MIT",
};

write(resolve(outDir, "package.json"), `${JSON.stringify(packageJson, null, 2)}\n`);
write(resolve(outDir, "README.md"), `# ${packageName}\n\n` +
  `Versioned TrUAPI protocol artifacts for products and hosts.\n\n` +
  `## Usage\n\n` +
  "```ts\n" +
  `import { manifest, methods, type TrUApiMethodName } from "${packageName}/v0.2";\n\n` +
  `const firstMethod: TrUApiMethodName = methods[0].name;\n` +
  `console.log(manifest.protocol.version, firstMethod);\n` +
  "```\n\n" +
  `The raw manifest is also available at \`${packageName}/v0.2/manifest.json\`.\n`);
write(resolve(outDir, "index.js"), `export * from "./v0.2/index.js";\nexport { default } from "./v0.2/index.js";\n`);
write(resolve(outDir, "index.d.ts"), `export * from "./v0.2/index.js";\nexport { default } from "./v0.2/index.js";\n`);
write(resolve(versionDir, "index.js"), jsModuleFor(manifest));
write(resolve(versionDir, "index.d.ts"), dts);
write(resolve(versionDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);

console.log(`Generated ${packageName}@${packageJson.version} artifacts from TrUAPI v${protocolVersion}.`);
