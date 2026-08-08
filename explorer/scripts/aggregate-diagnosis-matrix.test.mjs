import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const script = new URL("./aggregate-diagnosis-matrix.mjs", import.meta.url);

test("keeps SPA and Chat diagnosis reports in separate matrices", () => {
  const directory = mkdtempSync(join(tmpdir(), "truapi-compat-"));
  const output = join(directory, "compatibility.ts");

  writeFileSync(
    join(directory, "ios.md"),
    report("Truapi iOS Diagnosis", "Account/get_account"),
  );
  writeFileSync(
    join(directory, "chat-ios.md"),
    report("Truapi Chat Diagnosis", "Chat/post_message"),
  );

  execFileSync(process.execPath, [
    script.pathname,
    "--explorer-out",
    output,
    directory,
  ]);

  const generated = readFileSync(output, "utf8");
  const spa = generated.match(
    /export const compatibility: CompatibilityMatrix = ([\s\S]*?);\n\nexport const chatCompatibility/,
  )?.[1];
  const chat = generated.match(
    /export const chatCompatibility: CompatibilityMatrix = ([\s\S]*?);\n/,
  )?.[1];

  assert.ok(spa);
  assert.ok(chat);
  assert.deepEqual(JSON.parse(spa).hosts, [{ label: "iOS", mode: "iOS" }]);
  assert.deepEqual(JSON.parse(spa).methods.map(({ id }) => id), [
    "Account/get_account",
  ]);
  assert.deepEqual(JSON.parse(chat).hosts, [{ label: "iOS", mode: "iOS" }]);
  assert.deepEqual(JSON.parse(chat).methods.map(({ id }) => id), [
    "Chat/post_message",
  ]);
});

test("parent directory selects the matrix regardless of title", () => {
  const directory = mkdtempSync(join(tmpdir(), "truapi-compat-"));
  const output = join(directory, "compatibility.ts");

  mkdirSync(join(directory, "spa"));
  mkdirSync(join(directory, "chat"));
  writeFileSync(
    join(directory, "spa", "desktop.md"),
    report("Truapi Desktop Diagnosis", "Account/get_account"),
  );
  writeFileSync(
    join(directory, "chat", "desktop.md"),
    report("Truapi Desktop Diagnosis", "Chat/post_message"),
  );

  execFileSync(process.execPath, [
    script.pathname,
    "--explorer-out",
    output,
    directory,
  ]);

  const generated = readFileSync(output, "utf8");
  const spa = generated.match(
    /export const compatibility: CompatibilityMatrix = ([\s\S]*?);\n\nexport const chatCompatibility/,
  )?.[1];
  const chat = generated.match(
    /export const chatCompatibility: CompatibilityMatrix = ([\s\S]*?);\n/,
  )?.[1];

  assert.ok(spa);
  assert.ok(chat);
  assert.deepEqual(JSON.parse(spa).hosts, [
    { label: "Desktop", mode: "Desktop" },
  ]);
  assert.deepEqual(JSON.parse(spa).methods.map(({ id }) => id), [
    "Account/get_account",
  ]);
  assert.deepEqual(JSON.parse(chat).hosts, [
    { label: "Desktop", mode: "Desktop" },
  ]);
  assert.deepEqual(JSON.parse(chat).methods.map(({ id }) => id), [
    "Chat/post_message",
  ]);
});

function report(title, method) {
  return [
    `## ${title}`,
    "",
    "| Method | Status | Details |",
    "| --- | --- | --- |",
    `| \`${method}\` | ✅ | worked |`,
  ].join("\n");
}
