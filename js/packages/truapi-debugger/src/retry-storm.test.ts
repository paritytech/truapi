import { describe, expect, test } from "bun:test";

import { detectRetryStorms } from "./retry-storm.js";
import type { ObservedFrame } from "./observed-frame.js";
import type { WireTrace } from "./wire-debugger.js";

/**
 * A one-frame trace with a given op signature (`frameId`), start time, and
 * channel. Retry-storm is per-channel, so `channelId` defaults to a single
 * shared channel; multi-host tests pass distinct channels.
 */
function trace(
  requestId: string,
  frameId: number,
  startedAt: number,
  channelId = "c",
): WireTrace {
  const frame: ObservedFrame = {
    channelId,
    direction: "out",
    requestId,
    frameId,
    role: "request",
    byteLength: 0,
    timestamp: startedAt,
  };
  return { channelId, requestId, frames: [frame], startedAt, lastAt: startedAt };
}

/** requestIds of the stormed traces (result is keyed by the WireTrace object). */
function stormedIds(
  map: ReadonlyMap<WireTrace, readonly string[]>,
): string[] {
  return [...map.keys()].map((t) => t.requestId).sort();
}

describe("detectRetryStorms", () => {
  test("flags a burst of like ops in a short window", () => {
    const traces = [
      trace("a", 30, 0),
      trace("b", 30, 200),
      trace("c", 30, 400),
    ];
    const storms = detectRetryStorms(traces);
    expect(stormedIds(storms)).toEqual(["a", "b", "c"]);
    expect(storms.get(traces[0])).toEqual(["retry-storm"]);
  });

  test("does not flag a burst below the threshold", () => {
    const storms = detectRetryStorms([trace("a", 30, 0), trace("b", 30, 100)]);
    expect(storms.size).toBe(0);
  });

  test("does not flag like ops spread wider than the window", () => {
    const storms = detectRetryStorms([
      trace("a", 30, 0),
      trace("b", 30, 1500),
      trace("c", 30, 3000),
    ]);
    expect(storms.size).toBe(0);
  });

  test("groups by op signature — only the bursting method storms", () => {
    // Three createTransaction (id 30) inside 400ms = a storm; two getAccount
    // (id 22) far apart are not, even interleaved in time.
    const traces = [
      trace("sign-1", 30, 0),
      trace("get-1", 22, 50),
      trace("sign-2", 30, 150),
      trace("get-2", 22, 5000),
      trace("sign-3", 30, 300),
    ];
    const storms = detectRetryStorms(traces);
    expect(stormedIds(storms)).toEqual(["sign-1", "sign-2", "sign-3"]);
  });

  test("flags only the dense sub-window within a longer sparse run", () => {
    // Two early, far-apart ops then a tight burst of three: only the burst.
    const traces = [
      trace("x", 30, 0),
      trace("y", 30, 4000),
      trace("b1", 30, 8000),
      trace("b2", 30, 8300),
      trace("b3", 30, 8600),
    ];
    const storms = detectRetryStorms(traces);
    expect(stormedIds(storms)).toEqual(["b1", "b2", "b3"]);
  });

  test("honors custom window and burst thresholds", () => {
    const traces = [trace("a", 30, 0), trace("b", 30, 300)];
    // Default (minBurst 3) → nothing; minBurst 2 within 500ms → both.
    expect(detectRetryStorms(traces).size).toBe(0);
    const storms = detectRetryStorms(traces, { windowMs: 500, minBurst: 2 });
    expect(stormedIds(storms)).toEqual(["a", "b"]);
  });

  test("minBurst below 2 detects nothing", () => {
    const traces = [trace("a", 30, 0), trace("b", 30, 10)];
    expect(detectRetryStorms(traces, { minBurst: 1 }).size).toBe(0);
  });

  test("tolerates a frameless trace without throwing", () => {
    const empty: WireTrace = {
      channelId: "c",
      requestId: "empty",
      frames: [],
      startedAt: 0,
      lastAt: 0,
    };
    const traces = [
      empty,
      trace("a", 30, 0),
      trace("b", 30, 100),
      trace("c", 30, 200),
    ];
    const storms = detectRetryStorms(traces);
    expect(stormedIds(storms)).toEqual(["a", "b", "c"]);
    expect(storms.has(empty)).toBe(false);
  });

  test("is per-channel — two hosts each firing once is not a storm", () => {
    // Same requestId and frameId across two channels, all within the window,
    // but each channel fires the op only twice (< minBurst 3): no storm, and
    // the two channels are never merged into one burst.
    const traces = [
      trace("p:1", 30, 0, "hostA"),
      trace("p:1", 30, 50, "hostB"),
      trace("p:2", 30, 100, "hostA"),
      trace("p:2", 30, 150, "hostB"),
    ];
    expect(detectRetryStorms(traces).size).toBe(0);
  });

  test("flags a per-channel burst without pulling in the other channel", () => {
    // hostA hammers the op 3x in-window (storm); hostB fires it once (calm).
    const traces = [
      trace("p:1", 30, 0, "hostA"),
      trace("p:2", 30, 200, "hostA"),
      trace("p:1", 30, 250, "hostB"),
      trace("p:3", 30, 400, "hostA"),
    ];
    const storms = detectRetryStorms(traces);
    // Only hostA's three ops storm; hostB's p:1 does not, even though it shares
    // requestId "p:1" with a stormed hostA op.
    expect(storms.size).toBe(3);
    const stormedChannels = new Set([...storms.keys()].map((t) => t.channelId));
    expect([...stormedChannels]).toEqual(["hostA"]);
  });
});
