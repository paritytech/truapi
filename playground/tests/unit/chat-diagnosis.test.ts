import { describe, expect, test } from "bun:test";
import { CHAT_DIAGNOSIS_METHODS, ChatDiagnosis } from "../../worker/diagnosis";

describe("ChatDiagnosis", () => {
  test("keeps Chat methods ordered and renders a Chat-only report", () => {
    const diagnosis = new ChatDiagnosis();
    for (const id of CHAT_DIAGNOSIS_METHODS) {
      diagnosis.pass(id, "worked");
    }

    expect(diagnosis.isComplete()).toBe(true);
    expect(diagnosis.markdown()).toContain("## Truapi Chat Diagnosis");
    expect(diagnosis.markdown()).toContain("**5 success · 0 failed**");
    expect(diagnosis.markdown()).not.toContain("Storage/");
  });

  test("reports failures without preventing renderer output", () => {
    const diagnosis = new ChatDiagnosis();
    diagnosis.fail("Chat/create_room", new Error("room unavailable"));
    diagnosis.pass("Chat/create_room", "late success");

    expect(diagnosis.markdown()).toContain("❌ | room unavailable");
    expect(diagnosis.markdown()).not.toContain("late success");
    expect(diagnosis.rendererNode().tag).toBe("Column");
  });

  test("identifies the custom-rendered panel separately from the text report", () => {
    const diagnosis = new ChatDiagnosis();
    const renderer = JSON.stringify(diagnosis.rendererNode());

    expect(renderer).toContain("NATIVE CUSTOM MESSAGE");
    expect(renderer).toContain("Custom message rendered ✓");
    expect(renderer).toContain(
      "This panel is a live native renderer tree from TrUAPI Playground.",
    );
    expect(diagnosis.markdown()).not.toContain("Custom message rendered");
  });

  test("renders a copy action and reports clipboard fallback state", () => {
    const diagnosis = new ChatDiagnosis();
    const initial = JSON.stringify(diagnosis.rendererNode());

    expect(initial).toContain("Copy report");
    expect(initial).toContain("truapi-chat-diagnosis-copy");

    diagnosis.copyUnavailable();
    expect(JSON.stringify(diagnosis.rendererNode())).toContain(
      "long-press the report message below",
    );

    diagnosis.copied();
    expect(JSON.stringify(diagnosis.rendererNode())).toContain("Copied ✓");
  });
});
