import { describe, expect, test } from "bun:test";
import type { TrUApiClient } from "../../../../js/packages/truapi/src/index.ts";
import {
  ALLOCATION_APPROVAL_ACTION,
  VRF_APPROVAL_ACTION,
  actionLines,
  approvalLines,
  newLinesSince,
  runAutoSigningE2e,
} from "./auto-signing-e2e.ts";

describe("AutoSigning e2e approvals transcript", () => {
  test("splits the transcript into non-empty lines", () => {
    expect(approvalLines("approved allocate resources\n\n")).toEqual([
      "approved allocate resources",
    ]);
    expect(approvalLines("")).toEqual([]);
  });

  test("windows lines appended after a snapshot", () => {
    const before = approvalLines("approved allocate resources\n");
    const after = approvalLines(
      "approved allocate resources\napproved sign VRF transcript\n",
    );
    expect(newLinesSince(before, after)).toEqual([
      "approved sign VRF transcript",
    ]);
    expect(newLinesSince(after, after)).toEqual([]);
  });

  test("filters lines by confirmation action", () => {
    const lines = [
      "approved allocate resources",
      "denied sign VRF transcript",
      "approved submit preimage",
    ];
    expect(actionLines(lines, VRF_APPROVAL_ACTION)).toEqual([
      "denied sign VRF transcript",
    ]);
    expect(actionLines(lines, ALLOCATION_APPROVAL_ACTION)).toEqual([
      "approved allocate resources",
    ]);
  });

  test("skips without touching the client when the transcript is not wired", async () => {
    const row = await runAutoSigningE2e(
      undefined as unknown as TrUApiClient,
      "truapi-playground.dot",
      undefined,
    );
    expect(row.status).toBe("skipped");
    expect(row.output).toContain("TRUAPI_APPROVALS_LOG");
  });
});
