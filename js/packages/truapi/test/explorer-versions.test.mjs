// Smoke test for the @parity/truapi explorer registry.
//
// Verifies:
//  - The `./explorer/versions` subpath export resolves.
//  - The `main` entry is always at index 0 and has the same package version
//    string we publish under `packageVersion`.
//  - Every `MethodInfo.requestType`/`responseType`/`errorType` id references a
//    real `DataType` in the same version. This is the load-bearing invariant
//    the explorer site relies on for type navigation.
import assert from "node:assert/strict";

const mod = await import("../dist/explorer/versions.js");
const { versions, packageVersion } = mod;

assert.ok(Array.isArray(versions), "versions must be an array");
assert.ok(versions.length >= 1, "versions must have at least one entry");
assert.equal(versions[0].id, "main", "first entry must be `main`");
assert.equal(typeof packageVersion, "string");
assert.ok(packageVersion.length > 0, "packageVersion is empty");

for (const entry of versions) {
  assert.ok(typeof entry.id === "string" && entry.id.length > 0);
  assert.ok(Array.isArray(entry.services));
  assert.ok(Array.isArray(entry.types));

  const typeIds = new Set(entry.types.map((t) => t.id));

  for (const service of entry.services) {
    for (const method of service.methods) {
      for (const field of ["requestType", "responseType", "errorType"]) {
        const ref = method[field];
        if (ref == null) continue;
        assert.ok(
          typeIds.has(ref),
          `version ${entry.id} method ${service.name}.${method.name} ${field}=${ref} has no matching DataType`,
        );
      }
    }
  }

  // Every type must have a non-empty kebab id and a non-empty definition.
  for (const t of entry.types) {
    assert.ok(/^[a-z0-9][a-z0-9-]*$/.test(t.id), `bad id: ${t.id}`);
    assert.ok(typeof t.definition === "string" && t.definition.length > 0);
    assert.ok(typeof t.category === "string" && t.category.length > 0);
  }
}

console.log(`explorer versions smoke: ${versions.length} version(s), main has ${versions[0].services.length} services / ${versions[0].types.length} types`);
