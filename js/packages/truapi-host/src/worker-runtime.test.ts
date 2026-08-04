import { describe, expect, test } from "bun:test";

import { isLoopbackWsUrl } from "./worker-runtime.js";

describe("isLoopbackWsUrl", () => {
  test("accepts ws:// on every genuine loopback form", () => {
    expect(isLoopbackWsUrl("ws://localhost:9231")).toBe(true);
    expect(isLoopbackWsUrl("ws://127.0.0.1:9231")).toBe(true);
    expect(isLoopbackWsUrl("ws://127.5.6.7:9231")).toBe(true);
    expect(isLoopbackWsUrl("ws://[::1]:9231")).toBe(true);
  });

  test("rejects wss:// — the tap is ws-only, matching the native sink", () => {
    expect(isLoopbackWsUrl("wss://localhost:9231")).toBe(false);
    expect(isLoopbackWsUrl("wss://127.0.0.1:9231")).toBe(false);
  });

  test("rejects non-ws schemes and non-loopback hosts", () => {
    expect(isLoopbackWsUrl("http://127.0.0.1:9231")).toBe(false);
    expect(isLoopbackWsUrl("ws://192.0.2.1:9231")).toBe(false);
    expect(isLoopbackWsUrl("ws://example.com:9231")).toBe(false);
    expect(isLoopbackWsUrl("not a url")).toBe(false);
  });
});
