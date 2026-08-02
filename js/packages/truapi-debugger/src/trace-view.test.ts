// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

import { describe, expect, test } from "bun:test";
import type { ObservedFrame, FrameRole } from "./observed-frame.js";
import type { WireMethodInfo, WireTrace } from "./wire-debugger.js";
import { wireTraceToView } from "./trace-view.js";

function frame(
  role: FrameRole,
  frameId: number,
  timestamp: number,
  extra: Partial<ObservedFrame> = {},
): ObservedFrame {
  return {
    direction: role === "response" || role === "receive" ? "in" : "out",
    requestId: "req-1",
    frameId,
    role,
    byteLength: 8,
    timestamp,
    ...extra,
  };
}

function traceOf(frames: ObservedFrame[]): WireTrace {
  return {
    channelId: "test.dot",
    requestId: "req-1",
    frames,
    startedAt: frames[0]?.timestamp ?? 0,
    lastAt: frames[frames.length - 1]?.timestamp ?? 0,
  };
}

const methodNames: ReadonlyMap<number, WireMethodInfo> = new Map([
  [22, { method: "account.getAccount", kind: "request" }],
  [23, { method: "account.getAccount", kind: "response" }],
]);

describe("wireTraceToView", () => {
  test("resolves method names and per-frame latency from start", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1120)]),
      methodNames,
    );
    expect(view.frames.map((f) => f.method)).toEqual([
      "account.getAccount",
      "account.getAccount",
    ]);
    expect(view.frames[0].latencyFromStartMs).toBe(0);
    expect(view.frames[1].latencyFromStartMs).toBe(120);
    expect(view.durationMs).toBe(120);
  });

  test("a matched response carries a round-trip and no orphan badge", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1150)]),
      methodNames,
    );
    expect(view.frames[1].roundTripMs).toBe(150);
    expect(view.badges).toEqual([]);
  });

  test("a request with no response is orphaned", () => {
    const view = wireTraceToView(traceOf([frame("request", 22, 1000)]));
    expect(view.frames[0].badges).toContain("orphaned");
    expect(view.badges).toContain("orphaned");
  });

  test("a response with no request is orphaned", () => {
    const view = wireTraceToView(traceOf([frame("response", 23, 1000)]));
    expect(view.frames[0].badges).toContain("orphaned");
    expect(view.badges).toContain("orphaned");
  });

  test("subscription: one opener stays open across many receives, no orphans", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("receive", 41, 1200),
        frame("stop", 42, 1300),
      ]),
    );
    expect(view.badges).toEqual([]);
    // Each receive round-trips against the shared opener.
    expect(view.frames[1].roundTripMs).toBe(100);
    expect(view.frames[2].roundTripMs).toBe(200);
  });

  test("a live subscription (start + receives, no stop) is not orphaned", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("receive", 41, 1200),
      ]),
    );
    expect(view.badges).not.toContain("orphaned");
    expect(view.frames[0].badges).not.toContain("orphaned");
  });

  test("a subscribe that never delivered is orphaned", () => {
    const view = wireTraceToView(traceOf([frame("start", 40, 1000)]));
    expect(view.frames[0].badges).toContain("orphaned");
    expect(view.badges).toContain("orphaned");
  });

  test("a malformed frame flags both the frame and the op", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("malformed", -1, 1010)]),
    );
    expect(view.frames[1].badges).toContain("malformed");
    expect(view.badges).toContain("malformed");
  });

  test("retain-bytes drives the decodable flag", () => {
    const view = wireTraceToView(
      traceOf([
        frame("request", 22, 1000, { bytes: new Uint8Array([1, 2, 3]) }),
        frame("response", 23, 1100),
      ]),
      methodNames,
    );
    expect(view.frames[0].decodable).toBe(true);
    expect(view.frames[1].decodable).toBe(false);
  });

  test("caller-supplied op badges (retry-storm) are merged", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
      ["retry-storm"],
    );
    expect(view.badges).toContain("retry-storm");
  });
});
