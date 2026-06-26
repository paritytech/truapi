// Smoke test for the @parity/truapi explorer registry.
//
// Verifies that the published `main` entry leads the list, and that every
// `MethodInfo.requestType`/`responseType`/`errorType` id references a real
// `DataType` in the same version. That cross-reference is the load-bearing
// invariant the explorer site relies on for type navigation.

import { describe, expect, it } from "vitest";

import { packageVersion, versions } from "./versions.js";

describe("explorer versions registry", () => {
    it("exposes a populated versions array led by `main`", () => {
        expect(Array.isArray(versions)).toBe(true);
        expect(versions.length).toBeGreaterThanOrEqual(1);
        expect(versions[0].id).toBe("main");
        expect(typeof packageVersion).toBe("string");
        expect(packageVersion.length).toBeGreaterThan(0);
    });

    it.each(versions)("version $id resolves every method type reference to a DataType", (entry) => {
        expect(typeof entry.id).toBe("string");
        expect(entry.id.length).toBeGreaterThan(0);
        expect(Array.isArray(entry.services)).toBe(true);
        expect(Array.isArray(entry.types)).toBe(true);

        const typeIds = new Set(entry.types.map((t) => t.id));
        for (const service of entry.services) {
            for (const method of service.methods) {
                for (const field of ["requestType", "responseType", "errorType"] as const) {
                    const ref = method[field];
                    if (ref == null) continue;
                    expect(typeIds.has(ref), `${service.name}.${method.name} ${field}=${ref}`).toBe(
                        true,
                    );
                }
            }
        }
    });

    it.each(versions)(
        "version $id gives every type a kebab id, definition, and category",
        (entry) => {
            for (const t of entry.types) {
                expect(t.id).toMatch(/^[a-z0-9][a-z0-9-]*$/);
                expect(typeof t.definition).toBe("string");
                expect(t.definition.length).toBeGreaterThan(0);
                expect(typeof t.category).toBe("string");
                expect(t.category.length).toBeGreaterThan(0);
            }
        },
    );
});
