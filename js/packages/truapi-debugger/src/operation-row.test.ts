// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

import { describe, expect, test } from "bun:test";
import type { ObservedFrame, FrameRole } from "./observed-frame.js";
import type { WireMethodInfo, WireTrace } from "./wire-debugger.js";
import { wireTraceToView } from "./trace-view.js";
import { renderOperationRow } from "./trace-render.js";

function frame(
  role: FrameRole,
  frameId: number,
  timestamp: number,
): ObservedFrame {
  return {
    direction: role === "response" || role === "receive" ? "in" : "out",
    requestId: "p:1",
    frameId,
    role,
    byteLength: 8,
    timestamp,
  };
}

function traceOf(frames: ObservedFrame[]): WireTrace {
  return {
    channelId: "host-a.dot",
    requestId: "p:1",
    frames,
    startedAt: frames[0]?.timestamp ?? 0,
    lastAt: frames[frames.length - 1]?.timestamp ?? 0,
  };
}

const methodNames: ReadonlyMap<number, WireMethodInfo> = new Map([
  [22, { method: "account.getAccount", kind: "request" }],
  [23, { method: "account.getAccount", kind: "response" }],
  [40, { method: "account.connectionStatus", kind: "start" }],
  [41, { method: "account.connectionStatus", kind: "receive" }],
  [42, { method: "account.connectionStatus", kind: "stop" }],
]);

describe("renderOperationRow", () => {
  test("request/response op: method, frame count, duration, request glyph", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1120)]),
      methodNames,
    );
    const html = renderOperationRow(view);
    expect(html).toContain("account.getAccount");
    expect(html).toContain("2 frames");
    expect(html).toContain("120ms");
    expect(html).toContain("td-op-req");
    expect(html).toContain('data-request-id="p:1"');
    expect(html).not.toContain("td-op-live");
  });

  test("subscription with no stop is marked live", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("receive", 41, 1200),
      ]),
      methodNames,
    );
    const html = renderOperationRow(view);
    expect(html).toContain("td-op-sub");
    expect(html).toContain("td-op-live");
    expect(html).toContain("live");
  });

  test("subscription with a stop is not live", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("stop", 42, 1300),
      ]),
      methodNames,
    );
    const html = renderOperationRow(view);
    expect(html).toContain("td-op-sub");
    expect(html).not.toContain("td-op-live");
  });

  test("op badges render as chips (orphaned request)", () => {
    const view = wireTraceToView(traceOf([frame("request", 22, 1000)]), methodNames);
    const html = renderOperationRow(view);
    expect(html).toContain("td-badge-orphaned");
  });

  test("carries channelId as a data attribute when present", () => {
    const base = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    const view = { ...base, channelId: "host-a.dot" };
    const html = renderOperationRow(view);
    expect(html).toContain('data-channel-id="host-a.dot"');
  });

  test("omits data-channel-id when the vantage has no channel", () => {
    const base = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    const view = { ...base, channelId: undefined };
    expect(renderOperationRow(view)).not.toContain("data-channel-id");
  });

  test("payload-blind: never emits a decoded value", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    const html = renderOperationRow(view);
    expect(html).not.toContain("decode");
    expect(html).not.toContain("<pre");
  });

  test("escapes a hostile method/requestId", () => {
    const base = wireTraceToView(traceOf([frame("request", 22, 1000)]));
    const view = { ...base, requestId: '"><img src=x>' };
    const html = renderOperationRow(view);
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
  });
});
