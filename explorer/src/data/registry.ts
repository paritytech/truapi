import { versions as generatedVersions } from "@parity/truapi/explorer/registry";
import {
  getVersion as getGeneratedVersion,
  type ExplorerGroup,
  type ExplorerMethod,
  type ExplorerType,
  type ExplorerVersion,
} from "@parity/truapi/explorer/types";

export type Pattern =
  | "request-response"
  | "subscription"
  | "reverse-subscription";

export interface GroupDef {
  id: string;
  name: string;
  description: string;
  methods: string[];
}

export interface MethodDef {
  id: string;
  name: string;
  groupId: string;
  wireId: number;
  pattern: Pattern;
  description: string;
  request: string;
  response: string;
  errorType?: string;
  responseDescription?: string;
  notes?: string;
  usageExample?: string;
}

export interface DataType {
  id: string;
  name: string;
  category: string;
  definition: string;
  description: string;
  source?: string;
  fields?: Array<{ name: string; type: string; description: string }>;
  variants?: Array<{ name: string; type: string; description: string }>;
}

export interface VersionData {
  groups: GroupDef[];
  methods: MethodDef[];
  dataTypes: DataType[];
  getTypeById: (id: string) => DataType | undefined;
  getMethodById: (id: string) => MethodDef | undefined;
  getGroupById: (id: string) => GroupDef | undefined;
}

export interface VersionMeta {
  id: string;
  label: string;
  slug: string;
  status: "stable" | "preview";
  data: VersionData;
}

function pattern(method: ExplorerMethod): Pattern {
  if (method.name.startsWith("product_")) return "reverse-subscription";
  return method.pattern === "subscription"
    ? "subscription"
    : "request-response";
}

function toMethod(method: ExplorerMethod): MethodDef {
  return {
    id: method.id,
    name: method.name,
    groupId: method.groupId,
    wireId: method.wireId,
    pattern: pattern(method),
    description: method.description ?? "",
    request: method.request,
    response: method.response,
    errorType: method.errorType,
    usageExample: method.usageExample,
  };
}

function toGroup(group: ExplorerGroup): GroupDef {
  return {
    id: group.id,
    name: group.name,
    description: group.description ?? "",
    methods: group.methods,
  };
}

function toType(typeDef: ExplorerType): DataType {
  return {
    id: typeDef.id,
    name: typeDef.name,
    category: typeDef.category,
    definition: typeDef.definition,
    description: typeDef.description ?? "",
    source: typeDef.source,
    fields: typeDef.fields?.map((field) => ({
      name: field.name,
      type: field.type,
      description: field.description ?? "",
    })),
    variants: typeDef.variants?.map((variant) => ({
      name: variant.name,
      type: variant.type,
      description: variant.description ?? "",
    })),
  };
}

function toVersion(version: ExplorerVersion): VersionMeta {
  const groups = version.groups.map(toGroup);
  const methods = version.methods.map(toMethod);
  const dataTypes = version.dataTypes.map(toType);
  return {
    id: version.id,
    label: version.label,
    slug: version.slug,
    status: version.status,
    data: {
      groups,
      methods,
      dataTypes,
      getTypeById: (id) => dataTypes.find((typeDef) => typeDef.id === id),
      getMethodById: (id) => methods.find((method) => method.id === id),
      getGroupById: (id) => groups.find((group) => group.id === id),
    },
  };
}

export const versions: VersionMeta[] = generatedVersions.map(toVersion);
export const defaultVersion: VersionMeta = versions[versions.length - 1];

export function getVersion(slug: string): VersionMeta | undefined {
  const generated = getGeneratedVersion(generatedVersions, slug);
  if (!generated) return undefined;
  return versions.find((version) => version.slug === generated.slug);
}
