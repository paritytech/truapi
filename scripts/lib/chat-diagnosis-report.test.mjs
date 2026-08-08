import assert from "node:assert/strict";
import test from "node:test";
import {
  decodeTextMessage,
  labelChatDiagnosisReport,
} from "./chat-diagnosis-report.mjs";

test("decodes the multi-byte compact length used by a Chat report", () => {
  const text = `## Truapi Chat Diagnosis\n${"result ".repeat(20)}`;
  const body = Buffer.from(text);
  const compact = Buffer.alloc(2);
  compact.writeUInt16LE((body.length << 2) | 1);
  const encoded = Buffer.concat([Buffer.of(0), compact, body]);

  assert.equal(decodeTextMessage(encoded.toString("hex")), text);
  assert.equal(decodeTextMessage(Buffer.of(252).toString("hex")), undefined);
});

test("labels only a successful Chat-only report", () => {
  const report = [
    "## Truapi Chat Diagnosis",
    "",
    "**5 success · 0 failed**",
    "",
    "| Method | Status | Details |",
    "| --- | --- | --- |",
    "| `Chat/create_room` | ✅ | created |",
  ].join("\n");

  assert.match(
    labelChatDiagnosisReport(report, "iOS"),
    /^## Truapi iOS Chat Diagnosis/,
  );
  assert.match(
    labelChatDiagnosisReport(report.replace("5 success", "6 success"), "iOS"),
    /^## Truapi iOS Chat Diagnosis/,
  );
  assert.throws(() =>
    labelChatDiagnosisReport(report.replace("0 failed", "1 failed"), "iOS"),
  );
  assert.throws(() =>
    labelChatDiagnosisReport(report.replace("5 success", "0 success"), "iOS"),
  );
});
