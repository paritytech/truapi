export type ExplorerPattern = "unary" | "subscription";

export interface ExplorerField {
  name: string;
  type: string;
  description?: string;
}

export interface ExplorerVariant {
  name: string;
  type: string;
  description?: string;
}

export interface ExplorerType {
  id: string;
  name: string;
  category: string;
  definition: string;
  description?: string;
  source: string;
  fields?: ExplorerField[];
  variants?: ExplorerVariant[];
}

export interface ExplorerMethod {
  id: string;
  name: string;
  groupId: string;
  groupName: string;
  wireId: number;
  pattern: ExplorerPattern;
  request: string;
  response: string;
  errorType?: string;
  description?: string;
  usageExample?: string;
}

export interface ExplorerGroup {
  id: string;
  name: string;
  description?: string;
  methods: string[];
}

export interface ExplorerVersion {
  id: string;
  label: string;
  slug: string;
  status: "stable" | "preview";
  groups: ExplorerGroup[];
  methods: ExplorerMethod[];
  dataTypes: ExplorerType[];
}

export function getVersion(
  versions: readonly ExplorerVersion[],
  slug: string,
): ExplorerVersion | undefined {
  return versions.find((version) => version.slug === slug);
}

export function getTypeById(
  version: ExplorerVersion,
  id: string,
): ExplorerType | undefined {
  return version.dataTypes.find((typeDef) => typeDef.id === id);
}

export function getMethodById(
  version: ExplorerVersion,
  id: string,
): ExplorerMethod | undefined {
  return version.methods.find((method) => method.id === id);
}
