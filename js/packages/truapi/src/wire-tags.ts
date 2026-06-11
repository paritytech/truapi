// Reverse lookup over the generated wire table, used to label wire frames
// in debug logs. The frame-kind keys (`request`, `start`, ...) are derived
// from the table rows themselves rather than hardcoded.

import * as WireTable from "./generated/wire-table.js";

/**
 * Frame tag (`<method>_<kind>`, e.g. `system_handshake_request`) keyed by
 * wire discriminant, precomputed from the generated wire table.
 **/
export const WIRE_TAG_BY_ID: ReadonlyMap<number, string> = (() => {
  const map = new Map<number, string>();
  for (const [name, row] of Object.entries(WireTable)) {
    if (typeof row !== "object" || row === null) {
      continue;
    }
    for (const [kind, id] of Object.entries(row)) {
      if (typeof id === "number") {
        map.set(id, `${name.toLowerCase()}_${kind}`);
      }
    }
  }
  return map;
})();

/**
 * Human-readable label for a wire discriminant; `wire_<id>` when the id is
 * not in the wire table.
 **/
export function describeWireId(id: number): string {
  return WIRE_TAG_BY_ID.get(id) ?? `wire_${String(id)}`;
}
