import type { DataType, MethodInfo, ServiceInfo, VersionEntry } from "./types";
import { fallbackVersions } from "./fallback";

/**
 * Resolve the versions registry. Falls back to an empty stub if
 * `@parity/truapi/explorer/versions` is not yet on disk.
 */
async function loadVersions(): Promise<VersionEntry[]> {
  try {
    const mod = (await import("@parity/truapi/explorer/versions")) as {
      versions?: VersionEntry[];
    };
    if (mod.versions && mod.versions.length > 0) return mod.versions;
    return fallbackVersions;
  } catch {
    return fallbackVersions;
  }
}

export const versions: VersionEntry[] = await loadVersions();
if (versions.length === 0) {
  // Invariant: `loadVersions` always returns at least `fallbackVersions[0]`.
  // If this ever fires, the registry is corrupted and the explorer is unsafe
  // to render.
  throw new Error("explorer: versions registry is empty");
}

/** Convert "FooBar" or "snake_case" or "kebab-case" to camelCase. */
export function toCamel(name: string): string {
  const parts = name
    .replace(/[-_]+/g, " ")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  if (parts.length === 0) return name;
  return (
    parts[0] +
    parts
      .slice(1)
      .map((p) => p[0].toUpperCase() + p.slice(1))
      .join("")
  );
}

/** Title Case a hyphenated or snake_case category name. */
export function titleCase(name: string): string {
  return name
    .replace(/[-_]+/g, " ")
    .split(" ")
    .filter(Boolean)
    .map((p) => p[0].toUpperCase() + p.slice(1))
    .join(" ");
}

/** Find a version by id, falling back to the first available. */
export function findVersion(id: string | undefined): VersionEntry {
  if (id) {
    const v = versions.find((v) => v.id === id);
    if (v) return v;
  }
  return versions[0];
}

/** Find a method (and its owning service) by method name in a version. */
export function findMethod(
  version: VersionEntry,
  name: string,
): { service: ServiceInfo; method: MethodInfo } | null {
  for (const service of version.services) {
    const method = service.methods.find((m) => m.name === name);
    if (method) return { service, method };
  }
  return null;
}

/** Find a data type by id in a version. */
export function findType(version: VersionEntry, id: string): DataType | null {
  return version.types.find((t) => t.id === id) ?? null;
}

/** Methods in a version whose request/response/error matches the given type id. */
export function usedByType(
  version: VersionEntry,
  typeId: string,
): MethodInfo[] {
  const out: MethodInfo[] = [];
  for (const service of version.services) {
    for (const method of service.methods) {
      if (
        method.requestType === typeId ||
        method.responseType === typeId ||
        method.errorType === typeId
      ) {
        out.push(method);
      }
    }
  }
  return out;
}

/** Total number of methods across all services in a version. */
export function totalMethods(version: VersionEntry): number {
  return version.services.reduce((acc, s) => acc + s.methods.length, 0);
}

/** Filter methods by kind. */
export function methodsByKind(
  version: VersionEntry,
  kind: MethodInfo["type"],
): MethodInfo[] {
  const out: MethodInfo[] = [];
  for (const service of version.services) {
    for (const method of service.methods) {
      if (method.type === kind) out.push(method);
    }
  }
  return out;
}

/** Build `truapi.<service>.<methodName>(...)` for display. */
export function productFunction(
  service: ServiceInfo,
  method: MethodInfo,
): string {
  const svc = toCamel(service.name);
  const meth = toCamel(method.name);
  const arg = method.requestType ? "request" : "";
  return `truapi.${svc}.${meth}(${arg})`;
}
