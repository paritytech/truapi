// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

import { describe, expect, test } from "bun:test";
import type { FrameValueDetail } from "./decode.js";
import type { TraceView } from "./trace-view.js";
import { renderFrameValueDetail, renderTraceDetail } from "./trace-render.js";

const view: TraceView = {
  requestId: "req-1",
  startedAt: 1000,
  lastAt: 1150,
  durationMs: 150,
  frames: [
    {
      seq: 0,
      direction: "out",
      role: "request",
      method: "account.getAccount",
      frameId: 22,
      byteLength: 8,
      timestamp: 1000,
      latencyFromStartMs: 0,
      badges: [],
      decodable: true,
    },
    {
      seq: 1,
      direction: "in",
      role: "response",
      method: "account.getAccount",
      frameId: 23,
      byteLength: 40,
      timestamp: 1150,
      latencyFromStartMs: 150,
      roundTripMs: 150,
      badges: [],
      decodable: true,
    },
  ],
  badges: [],
};

describe("renderTraceDetail", () => {
  test("renders the frame sequence with method, bytes, and round-trip", () => {
    const html = renderTraceDetail(view);
    expect(html).toContain("account.getAccount");
    expect(html).toContain("40B");
    expect(html).toContain("150ms");
    expect(html).toContain('data-seq="1"');
  });

  test("is payload-blind by default: no decode control", () => {
    const html = renderTraceDetail(view);
    expect(html).not.toContain("decode payload");
  });

  test("offers a decode control per decodable frame when opted in", () => {
    const html = renderTraceDetail(view, { offerDecode: true });
    expect(html).toContain("td-frame-decode-btn");
    expect(html).toContain("decode payload");
  });

  test("renders a resolved decoded value in place of the control", () => {
    const decoded = new Map<number, FrameValueDetail>([
      [1, { kind: "decoded", value: { free: 42 } }],
    ]);
    const html = renderTraceDetail(view, { offerDecode: true, decoded });
    expect(html).toContain("&quot;free&quot;: 42");
  });

  test("a sensitive frame renders a redacted state, never the value", () => {
    const decoded = new Map<number, FrameValueDetail>([
      [0, { kind: "redacted", reason: "sensitive method", byteLength: 96 }],
    ]);
    const html = renderTraceDetail(view, { offerDecode: true, decoded });
    expect(html).toContain("redacted");
    expect(html).toContain("96B withheld");
    expect(html).not.toContain("free");
  });

  test("escapes wire-sourced strings", () => {
    const evil: TraceView = {
      ...view,
      requestId: '<img src=x onerror="alert(1)">',
      frames: [],
    };
    const html = renderTraceDetail(evil);
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
  });

  test("op-level badges appear in the header", () => {
    const html = renderTraceDetail({ ...view, badges: ["orphaned", "retry-storm"] });
    expect(html).toContain("td-badge-orphaned");
    expect(html).toContain("retry storm");
  });
});

describe("renderFrameValueDetail", () => {
  test("bytes-only never shows a payload", () => {
    const html = renderFrameValueDetail({ kind: "bytes", byteLength: 12 });
    expect(html).toContain("12B");
    expect(html).toContain("payload not shown");
  });
});
