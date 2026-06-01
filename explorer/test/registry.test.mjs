// Regression test for method lookup and routing in the explorer registry.
//
// Guards issue #155: two methods that share a name under different services
// (e.g. `Theme/subscribe` and `Statement Store/subscribe`) must resolve to
// distinct method pages. `findMethod` is qualified by service, and `methodPath`
// encodes each segment so service names with spaces round-trip through the URL.
import assert from "node:assert/strict";
import { findMethod, methodPath, usedByType } from "../src/data/registry.ts";

/** Minimal version with the duplicate-name collision from issue #155. */
const version = {
  id: "main",
  types: [{ id: "Empty", name: "Empty", category: "core", definition: "" }],
  services: [
    {
      name: "Theme",
      methods: [
        { name: "subscribe", type: "subscription", requestType: "Empty" },
      ],
    },
    {
      name: "Statement Store",
      methods: [
        { name: "subscribe", type: "subscription", requestType: "Empty" },
        { name: "submit", type: "unary" },
      ],
    },
  ],
};

const theme = version.services[0];
const store = version.services[1];

// Same method name under two services resolves to the correct owning service.
assert.deepEqual(findMethod(version, "Theme", "subscribe"), {
  service: theme,
  method: theme.methods[0],
});
assert.deepEqual(findMethod(version, "Statement Store", "subscribe"), {
  service: store,
  method: store.methods[0],
});

// The two `subscribe` lookups are genuinely distinct (the core regression).
assert.notEqual(
  findMethod(version, "Theme", "subscribe").service,
  findMethod(version, "Statement Store", "subscribe").service,
);

// Misses return null rather than falling through to another service.
assert.equal(findMethod(version, "Nope", "subscribe"), null);
assert.equal(findMethod(version, "Theme", "submit"), null);

// Route segments are URL-encoded; a service name with a space round-trips.
assert.equal(
  methodPath("main", "Statement Store", "subscribe"),
  "/v/main/method/Statement%20Store/subscribe",
);
const path = methodPath("main", "Statement Store", "subscribe");
const [, , , , svc, meth] = path.split("/");
assert.equal(decodeURIComponent(svc), "Statement Store");
assert.equal(decodeURIComponent(meth), "subscribe");

// `usedByType` keeps each method qualified by its service, so same-named
// methods referencing the same type stay distinct on the type-detail page.
assert.deepEqual(usedByType(version, "Empty"), [
  { service: theme, method: theme.methods[0] },
  { service: store, method: store.methods[0] },
]);

console.log("registry: all assertions passed");
