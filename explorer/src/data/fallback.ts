import type { VersionEntry } from "./types";

/** Empty registry shipped when `@parity/truapi/explorer/versions` is unavailable. */
export const fallbackVersions: VersionEntry[] = [
  { id: "main", services: [], types: [] },
];
