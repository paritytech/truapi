import assert from "node:assert/strict";

import { createClient } from "../src/generated/client.ts";
import { services } from "../src/playground/services.ts";

const ENCODED = Symbol("payload encoded");

const serviceFields = {
  "Account Management": "accountManagement",
  "Chain Interaction": "chainInteraction",
  Chat: "chat",
  "Entropy Derivation": "entropyDerivation",
  "Host Theme": "hostTheme",
  "JSON-RPC": "jsonRpc",
  "Local Storage": "localStorage",
  Payment: "payment",
  Permissions: "permissions",
  Preimage: "preimage",
  "Resource Allocation": "resourceAllocation",
  Signing: "signing",
  "Statement Store": "statementStore",
  System: "system",
};

function methodField(methodName) {
  const withoutPrefix = methodName.replace(/^(host|remote|product)_/, "");
  return withoutPrefix.replace(/_([a-z])/g, (_, ch) => ch.toUpperCase());
}

function normalizeForScale(value) {
  if (value === null) return undefined;
  if (typeof value === "string" && /^-?\d+n$/.test(value)) {
    return BigInt(value.slice(0, -1));
  }
  if (Array.isArray(value)) return value.map(normalizeForScale);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [
        key,
        normalizeForScale(nested),
      ]),
    );
  }
  return value;
}

const client = createClient({
  request() {
    throw ENCODED;
  },
  subscribeRaw() {
    return { subscriptionId: "test-subscription", unsubscribe() {} };
  },
  dispose() {},
});

for (const service of services) {
  const serviceField = serviceFields[service.name];
  assert.ok(serviceField, `missing client service mapping for ${service.name}`);
  const serviceClient = client[serviceField];

  for (const method of service.methods) {
    if (!method.defaultRequest) continue;

    const parsed = JSON.parse(method.defaultRequest);
    const request = normalizeForScale(parsed);
    const fn = serviceClient[methodField(method.name)];
    assert.equal(typeof fn, "function", `missing client method ${method.name}`);

    const label = `${service.name}/${method.name}`;
    if (method.type === "subscription") {
      assert.doesNotThrow(() => fn.call(serviceClient, { request }), label);
    } else {
      await assert.rejects(
        () => fn.call(serviceClient, request),
        (error) => error === ENCODED,
        label,
      );
    }
  }
}

console.log("playground default requests encode successfully");
