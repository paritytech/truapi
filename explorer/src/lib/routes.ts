import type { MethodDef } from "../data/registry";

export function methodRoutePath(method: Pick<MethodDef, "groupId" | "id">) {
  return `/method/${method.groupId}/${method.id}`;
}

export function versionedMethodRoutePath(
  versionPrefix: string,
  method: Pick<MethodDef, "groupId" | "id">,
) {
  return `${versionPrefix}${methodRoutePath(method)}`;
}
