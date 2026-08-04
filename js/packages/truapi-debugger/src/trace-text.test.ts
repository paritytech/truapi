import { describe, expect, test } from "bun:test";

import { buildTraceView } from "./trace-view.js";
import {
  formatFrameValue,
  formatOpRow,
  formatStats,
  type CliStats,
} from "./trace-text.js";

const stats: CliStats = {
  ops: 2,
  frames: 5,
  bytes: 40,
  subscriptions: 1,
  liveSubscriptions: 1,
  malformed: 0,
  orphaned: 1,
  retryStorms: 0,
  truncated: 0,
  evictedTraces: 0,
  droppedByHost: 0,
  codecMismatch: false,
  sensitive: 1,
  out: 3,
  in: 2,
  avgDurationMs: 12,
  maxDurationMs: 30,
  topMethods: [],
};

describe("formatStats", () => {
  test("renders counts and surfaces sensitive + orphaned", () => {
    const s = formatStats(stats);
    expect(s).toContain("ops");
    expect(s).toContain("sensitive");
    expect(s).toContain("orphaned");
  });
});

describe("formatOpRow", () => {
  test("shows method, requestId, and a lock for a sensitive op", () => {
    const view = buildTraceView({
      requestId: "p:1",
      startedAt: 0,
      lastAt: 12,
      frames: [
        {
          direction: "out",
          role: "request",
          method: "account.getAccount",
          frameId: 22,
          byteLength: 20,
          timestamp: 0,
          decodable: false,
          sensitive: true,
        },
        {
          direction: "in",
          role: "response",
          method: "account.getAccount",
          frameId: 23,
          byteLength: 20,
          timestamp: 12,
          decodable: false,
          sensitive: true,
        },
      ],
    });
    const row = formatOpRow(view);
    expect(row).toContain("account.getAccount");
    expect(row).toContain("p:1");
    expect(row).toContain("\u{1f512}");
  });
});

describe("formatFrameValue", () => {
  test("redacted never shows a value", () => {
    expect(
      formatFrameValue({
        kind: "redacted",
        reason: "sensitive method",
        byteLength: 64,
      }),
    ).toContain("redacted");
  });

  test("a revealed sensitive value is flagged dev-only, and still shows content", () => {
    const out = formatFrameValue({
      kind: "decoded",
      value: { free: 42 },
      sensitive: true,
    });
    expect(out).toContain("revealed sensitive material");
    expect(out).toContain("42");
  });

  test("bytes-only shows no payload", () => {
    expect(formatFrameValue({ kind: "bytes", byteLength: 8 })).toContain(
      "payload not shown",
    );
  });
});
