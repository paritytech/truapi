import { describe, expect, test } from "bun:test";
import { renderDiagnosisMarkdown } from "../../shared/diagnosis";

describe("renderDiagnosisMarkdown", () => {
  test("renders deterministic ordered rows and escapes details", () => {
    expect(
      renderDiagnosisMarkdown(
        [
          { id: "Chat/create_room", status: "pass", details: "New | Exists" },
          { id: "Chat/post_message", status: "fail", details: "not stored" },
        ],
        { title: "Truapi iOS Chat Diagnosis" },
      ),
    ).toBe(
      [
        "## Truapi iOS Chat Diagnosis",
        "",
        "**1 success · 1 failed**",
        "",
        "| Method | Status | Details |",
        "| --- | --- | --- |",
        "| `Chat/create_room` | ✅ | New \\| Exists |",
        "| `Chat/post_message` | ❌ | not stored |",
      ].join("\n"),
    );
  });

  test("drops only successful details for compact reports", () => {
    const report = renderDiagnosisMarkdown(
      [
        { id: "Chat/create_room", status: "pass", details: "created" },
        { id: "Chat/post_message", status: "fail", details: "failed" },
      ],
      { title: "Truapi Chat Diagnosis", dropSuccessDetails: true },
    );

    expect(report).toContain("| `Chat/create_room` | ✅ |  |");
    expect(report).toContain("| `Chat/post_message` | ❌ | failed |");
  });
});
