import { describe, expect, test } from "bun:test";

import { createWireDebugger } from "./wire-debugger.js";
import type { ObservedFrame } from "./observed-frame.js";

/** A minimal observed frame; only the fields the trace engine keys/groups on matter. */
function frame(
  channelId: string,
  requestId: string,
  frameId: number,
  timestamp: number,
): ObservedFrame {
  return {
    channelId,
    direction: "out",
    requestId,
    frameId,
    role: "unknown",
    byteLength: 1,
    timestamp,
  };
}

describe("createWireDebugger grouping", () => {
  test("accumulates every frame of one op under (channel, requestId)", () => {
    // Regression guard: the request and its response share a channel + requestId
    // and must land in ONE trace. (A bug where lookup used the composite key but
    // re-insert used the bare requestId made every frame spawn a new 1-frame
    // trace, so nothing ever paired.)
    const wd = createWireDebugger({ sink: () => {} });
    wd.observe(frame("app.dot", "p:1", 22, 1)); // request
    wd.observe(frame("app.dot", "p:1", 23, 2)); // response

    const traces = wd.traces();
    expect(traces).toHaveLength(1);
    expect(traces[0].frames).toHaveLength(2);
    expect(traces[0].frames.map((f) => f.frameId)).toEqual([22, 23]);
    expect(traces[0].channelId).toBe("app.dot");
  });

  test("a long subscription keeps accumulating under one trace", () => {
    const wd = createWireDebugger({ sink: () => {} });
    wd.observe(frame("app.dot", "s:7", 18, 1)); // start
    for (let i = 0; i < 50; i++) {
      wd.observe(frame("app.dot", "s:7", 21, 2 + i)); // receive
    }
    const traces = wd.traces();
    expect(traces).toHaveLength(1);
    expect(traces[0].frames).toHaveLength(51);
  });

  test("does not merge the same requestId across channels", () => {
    const wd = createWireDebugger({ sink: () => {} });
    wd.observe(frame("hostA.dot", "p:1", 22, 1));
    wd.observe(frame("hostB.dot", "p:1", 80, 2));

    const traces = wd.traces();
    expect(traces).toHaveLength(2);
    expect(new Set(traces.map((t) => t.channelId))).toEqual(
      new Set(["hostA.dot", "hostB.dot"]),
    );
  });

  test("trace() resolves by requestId, disambiguated by channel", () => {
    const wd = createWireDebugger({ sink: () => {} });
    wd.observe(frame("hostA.dot", "p:1", 22, 1));
    wd.observe(frame("hostB.dot", "p:1", 80, 2));

    // With a channel, the exact trace; without, the first match by requestId.
    expect(wd.trace("p:1", "hostB.dot")?.frames[0].frameId).toBe(80);
    expect(wd.trace("p:1", "hostA.dot")?.frames[0].frameId).toBe(22);
    expect(wd.trace("p:1")).toBeDefined();
    expect(wd.trace("nope")).toBeUndefined();
  });

  test("tracesForChannel filters to one channel", () => {
    const wd = createWireDebugger({ sink: () => {} });
    wd.observe(frame("hostA.dot", "p:1", 22, 1));
    wd.observe(frame("hostA.dot", "p:2", 24, 2));
    wd.observe(frame("hostB.dot", "p:1", 80, 3));

    expect(wd.tracesForChannel("hostA.dot")).toHaveLength(2);
    expect(wd.tracesForChannel("hostB.dot")).toHaveLength(1);
    expect(wd.tracesForChannel("absent.dot")).toHaveLength(0);
  });
});
