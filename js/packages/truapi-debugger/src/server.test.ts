import { expect, test } from "bun:test";

import {
  encodeWireMessage,
  TRUAPI_WIRE_SCHEMA_HASH,
  VersionedHostSignRawRequest,
} from "@parity/truapi";
import * as W from "@parity/truapi/wire-table";

import { isLoopbackDebugHost, startDebugServer } from "./server.js";

interface TraceFrameView {
  direction: string;
  frameId: number;
  method?: string;
  byteLength: number;
}
interface TraceView {
  requestId: string;
  frames: TraceFrameView[];
}

/** base64 of a wire message for `frameId` carrying `value` as its payload. */
function encodeFrame(requestId: string, frameId: number, value: Uint8Array): string {
  const encoded = encodeWireMessage({ requestId, payload: { id: frameId, value } });
  if (encoded.isErr()) throw encoded.error;
  return Buffer.from(encoded.value).toString("base64");
}

/**
 * base64 of a real, decodable sign-raw request wire message. Carries a
 * recognizable `dotNsIdentifier` ("alice.dot") in its decoded value so a test
 * can prove the value surfaced — this debugger decodes it like any other frame.
 */
function signFrame(requestId: string): string {
  const value = VersionedHostSignRawRequest.enc({
    tag: "V1",
    value: {
      account: {
        dotNsIdentifier: "alice.dot",
        derivationIndex: { tag: "Left", value: 0 },
      },
      payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
    },
  });
  const encoded = encodeWireMessage({
    requestId,
    payload: { id: W.SIGNING_SIGN_RAW.request, value },
  });
  if (encoded.isErr()) throw encoded.error;
  return Buffer.from(encoded.value).toString("base64");
}

/** Open a WS to the server, send one envelope, wait until `/traces` is non-empty. */
async function streamFrame(
  base: string,
  port: number,
  frame: string,
  dir: "in" | "out" = "out",
): Promise<TraceView[]> {
  const ws = new WebSocket(`ws://localhost:${port}`);
  await new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = () => reject(new Error("ws failed to open"));
  });
  ws.send(
    JSON.stringify({
      channelId: "myapp.dot",
      dir,
      frame,
      schema: TRUAPI_WIRE_SCHEMA_HASH,
    }),
  );
  let traces: TraceView[] = [];
  for (let i = 0; i < 50 && traces.length === 0; i++) {
    traces = (await (await fetch(`${base}/traces`)).json()) as TraceView[];
    if (traces.length === 0) await new Promise((r) => setTimeout(r, 20));
  }
  ws.close();
  return traces;
}

test("decodes and groups a frame a host streams over the WS", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const encoded = encodeWireMessage({
      requestId: "p:1",
      payload: { id: W.SYSTEM_HANDSHAKE.request, value: new Uint8Array([1, 2, 3]) },
    });
    if (encoded.isErr()) throw encoded.error;
    const frame = Buffer.from(encoded.value).toString("base64");

    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed to open"));
    });
    ws.send(
      JSON.stringify({
        channelId: "myapp.dot",
        dir: "out",
        frame,
        schema: TRUAPI_WIRE_SCHEMA_HASH,
      }),
    );

    let traces: TraceView[] = [];
    for (let i = 0; i < 50 && traces.length === 0; i++) {
      traces = (await (await fetch(`${base}/traces`)).json()) as TraceView[];
      if (traces.length === 0) await new Promise((r) => setTimeout(r, 20));
    }
    ws.close();

    expect(traces).toHaveLength(1);
    expect(traces[0].requestId).toBe("p:1");
    expect(traces[0].frames[0].direction).toBe("out");
    expect(traces[0].frames[0].frameId).toBe(W.SYSTEM_HANDSHAKE.request);
    // The method map resolves the wire id to a dotted name for the view.
    expect(typeof traces[0].frames[0].method).toBe("string");
  } finally {
    server.stop();
  }
});

test("the inspector page is served at /", async () => {
  const server = startDebugServer({ port: 0 });
  try {
    const res = await fetch(`http://localhost:${server.port}/`);
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/html");
    const html = await res.text();
    expect(html).toContain("TrUAPI Wire Inspector");
    // The shell fetches the shared fragments, not a bespoke renderer.
    expect(html).toContain("/op-list");
    expect(html).toContain("/op?id=");
  } finally {
    server.stop();
  }
});

test("/op-list renders one shared row per op, payload-blind", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);
    const html = await (await fetch(`${base}/op-list`)).text();
    expect(html).toContain("td-op");
    expect(html).toContain('data-request-id="p:1"');
    // Subscription start, no stop yet: marked live. And never a value.
    expect(html).toContain("td-op-sub");
    expect(html).not.toContain("V1");
  } finally {
    server.stop();
  }
});

test("/op renders the drill-down for one op; unknown id degrades", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.SYSTEM_HANDSHAKE.request, new Uint8Array([1]));
    await streamFrame(base, server.port, frame);
    const ok = await (await fetch(`${base}/op?id=p:1`)).text();
    expect(ok).toContain("td-trace");
    expect(ok).toContain('data-request-id="p:1"');
    const missing = await (await fetch(`${base}/op?id=nope`)).text();
    expect(missing).toContain("not found");
  } finally {
    server.stop();
  }
});

test("/channels reports the hosts that have dialed in", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.SYSTEM_HANDSHAKE.request, new Uint8Array([1]));
    await streamFrame(base, server.port, frame);
    const data = (await (await fetch(`${base}/channels`)).json()) as {
      sockets: number;
      channels: {
        channelId: string;
        firstSeen: number;
        lastSeen: number;
        frameCount: number;
        connected: boolean;
      }[];
    };
    const ch = data.channels.find((c) => c.channelId === "myapp.dot");
    expect(ch).toBeDefined();
    expect(ch?.frameCount).toBeGreaterThanOrEqual(1);
    expect(ch?.connected).toBe(true);
    expect(ch?.firstSeen).toBeLessThanOrEqual(ch?.lastSeen ?? 0);
  } finally {
    server.stop();
  }
});

test("/traces is byte- and value-free even with value decode on", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // A decodable, non-sensitive frame: `connection-status.subscribe` start is
    // `V1(void)` = a single 0x00 byte, which the generated table decodes to a
    // `{ tag: "V1" }` value - a value that must never appear in `/traces`.
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    const traces = await streamFrame(base, server.port, frame);
    expect(traces).toHaveLength(1);

    const raw = await (await fetch(`${base}/traces`)).text();
    // No payload-bearing keys and no decoded content leak into the trace list.
    for (const banned of ['"bytes"', '"value"', '"decoded"', '"tag"', "V1"]) {
      expect(raw).not.toContain(banned);
    }
  } finally {
    server.stop();
  }
});

test("/stats is byte- and value-free even with value decode on", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // The same decodable, non-sensitive frame as the /traces test: its decoded
    // value is `{ tag: "V1" }`. The aggregate must report only counts - its
    // `bytes` field is a summed byte *length*, never a raw or decoded payload.
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);

    const raw = await (await fetch(`${base}/stats`)).text();
    // No decoded content and no raw-payload hex leaks into the aggregate.
    for (const banned of ['"value"', '"decoded"', '"tag"', "V1", "0x"]) {
      expect(raw).not.toContain(banned);
    }
    // The aggregate is present, and `bytes` is a summed length (here 1B), a count.
    const stats = JSON.parse(raw) as {
      ops: number;
      frames: number;
      bytes: number;
    };
    expect(stats.ops).toBe(1);
    expect(stats.frames).toBe(1);
    expect(stats.bytes).toBe(1);
  } finally {
    server.stop();
  }
});

test("/frame decodes a non-sensitive frame by default; decodeValues:false reports bytes", async () => {
  const frame = encodeFrame(
    "p:1",
    W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
    new Uint8Array([0]),
  );

  // Default (dev-only tool): decode is on, so the drill-down surfaces the value.
  const on = startDebugServer({ port: 0 });
  try {
    expect(on.decodeValues).toBe(true);
    const baseOn = `http://localhost:${on.port}`;
    await streamFrame(baseOn, on.port, frame);
    const detail = await (await fetch(`${baseOn}/frame?id=p:1&i=0`)).json();
    expect(detail.kind).toBe("decoded");
    expect(detail.value?.tag).toBe("V1");
  } finally {
    on.stop();
  }

  // `decodeValues: false` (still supported, for demos/tests): byte length only.
  const off = startDebugServer({ port: 0, decodeValues: false });
  try {
    expect(off.decodeValues).toBe(false);
    const baseOff = `http://localhost:${off.port}`;
    await streamFrame(baseOff, off.port, frame);
    const detail = await (await fetch(`${baseOff}/frame?id=p:1&i=0`)).json();
    expect(detail.kind).toBe("bytes");
    expect(detail.byteLength).toBe(1);
  } finally {
    off.stop();
  }
});

test("a signing frame decodes like any other; /traces never carries its bytes", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    await streamFrame(base, server.port, signFrame("p:sign"));

    // Dev-only tool: no denylist, so the frame decodes and its value surfaces.
    const detail = await (await fetch(`${base}/frame?id=p:sign&i=0`)).json();
    expect(detail.kind).toBe("decoded");
    expect(JSON.stringify(detail.value)).toContain("alice.dot");
    // The decoded result never carries a "sensitive"/"redacted" marker any more.
    expect(detail.sensitive).toBeUndefined();

    // The payload-blind grouping invariant still holds: /traces never serializes
    // the raw or decoded bytes, only the /frame drill-down does.
    const raw = await (await fetch(`${base}/traces`)).text();
    expect(raw).not.toContain("deadbeef");
    expect(raw).not.toContain("alice.dot");
  } finally {
    server.stop();
  }
});

test("/view renders the shared drill-down with decoded values by default", async () => {
  // Default (dev-only tool): decode is on, so the drill-down renders each
  // frame's value inline — no click-to-decode control.
  const server = startDebugServer({ port: 0 });
  try {
    const base = `http://localhost:${server.port}`;
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);
    const html = await (await fetch(`${base}/view`)).text();
    // Shared-renderer markup, not the old table.
    expect(html).toContain("td-trace");
    expect(html).toContain("td-frame");
    expect(html).toContain('data-request-id="p:1"');
    // Values render inline; the click-to-decode control is gone.
    expect(html).toContain("td-frame-payload");
    expect(html).not.toContain("td-frame-decode-btn");
    expect(html).not.toContain("decode payload");
  } finally {
    server.stop();
  }
});

test("/view is payload-blind when decode is off", async () => {
  const off = startDebugServer({ port: 0, decodeValues: false });
  try {
    const base = `http://localhost:${off.port}`;
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, off.port, frame);
    const html = await (await fetch(`${base}/view`)).text();
    expect(html).toContain('data-request-id="p:1"');
    // No payload column at all, and no decode control.
    expect(html).not.toContain("td-frame-payload");
    expect(html).not.toContain("td-frame-decode-btn");
  } finally {
    off.stop();
  }
});

test("/op decodes every frame inline via the real decodeTraceFrames path", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // A real sign-raw request whose decoded value carries "alice.dot".
    await streamFrame(base, server.port, signFrame("p:sign"));

    // The op drill-down renders the decoded value inline — proving the
    // session → decodeTraceFrames → renderer wiring, not just structural markup.
    const html = await (
      await fetch(`${base}/op?id=p:sign&channel=myapp.dot&gen=0`)
    ).text();
    expect(html).toContain("td-frame-decoded");
    expect(html).toContain("alice.dot");
    // Inline, not behind a control, and nothing withheld.
    expect(html).not.toContain("td-frame-decode-btn");
    expect(html).not.toContain("redacted");
  } finally {
    server.stop();
  }
});

test("/op refuses to decode a codec-mismatched (untrusted) channel", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // Stream a frame with a wrong wire schema hash: the channel is untrusted.
    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed"));
    });
    ws.send(
      JSON.stringify({
        channelId: "drift.dot",
        dir: "out",
        frame: signFrame("p:sign"),
        schema: "0000000000000000",
      }),
    );
    for (let i = 0; i < 50; i++) {
      const t = (await (await fetch(`${base}/traces`)).json()) as TraceView[];
      if (t.length > 0) break;
      await new Promise((r) => setTimeout(r, 20));
    }
    ws.close();

    const html = await (
      await fetch(`${base}/op?id=p:sign&channel=drift.dot&gen=0`)
    ).text();
    // Grouped and shown, but no decoded value for the untrusted channel.
    expect(html).toContain('data-request-id="p:sign"');
    expect(html).not.toContain("alice.dot");
    expect(html).toContain("payload not shown");
  } finally {
    server.stop();
  }
});

test("/frame validates its params and 404s an unknown frame", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    expect((await fetch(`${base}/frame`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=notint`)).status).toBe(400);
    // Empty `?i=` must 400, not resolve frame 0 (Number("") === 0).
    expect((await fetch(`${base}/frame?id=x&i=`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=%20`)).status).toBe(400);
    // Same coercion on `?gen=`: empty/whitespace/non-int must 400, not resolve
    // generation 0 (the oldest recycled op) with a 200.
    expect((await fetch(`${base}/frame?id=x&i=0&gen=`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=0&gen=%20`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=0&gen=notint`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=missing&i=0`)).status).toBe(404);
  } finally {
    server.stop();
  }
});

test("a codec-mismatched host is banner-flagged and its frames refuse to decode", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array([0]),
    );
    // Stream one frame declaring a codec this debugger can't decode against.
    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed to open"));
    });
    ws.send(
      JSON.stringify({ v: 1, codec: 999, channelId: "old.dot", dir: "out", frame }),
    );
    // Wait until the frame is grouped (payload-blind grouping still happens).
    for (let i = 0; i < 50; i++) {
      const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
      if (traces.length > 0) break;
      await new Promise((r) => setTimeout(r, 20));
    }
    ws.close();

    // /channels banners the mismatch.
    const channels = await (await fetch(`${base}/channels`)).json();
    expect(channels.codecMismatch).toBe(true);
    // Decode is refused (409) for that host's frames — never resolved against the
    // wrong contract.
    const refused = await fetch(`${base}/frame?id=p:1&i=0&channel=old.dot`);
    expect(refused.status).toBe(409);
  } finally {
    server.stop();
  }
});

test("a wrong-schema or unstamped host refuses to decode, but still groups", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array([0]),
    );
    const stream = async (envelope: Record<string, unknown>): Promise<void> => {
      const ws = new WebSocket(`ws://localhost:${server.port}`);
      await new Promise<void>((resolve, reject) => {
        ws.onopen = () => resolve();
        ws.onerror = () => reject(new Error("ws failed to open"));
      });
      const want = ((await (await fetch(`${base}/traces`)).json()) as unknown[])
        .length;
      ws.send(JSON.stringify(envelope));
      for (let i = 0; i < 50; i++) {
        const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
        if (traces.length > want) break;
        await new Promise((r) => setTimeout(r, 20));
      }
      ws.close();
    };
    // A frame stamping a wire schema this debugger can't decode against (the
    // codec number alone is unchanged) must be refused, never resolved against
    // the wrong contract - the case a coarse codec check misses.
    await stream({
      channelId: "stale.dot",
      dir: "out",
      frame,
      codec: 1,
      schema: "deadbeefdeadbeef",
    });
    expect(
      (await fetch(`${base}/frame?id=p:1&i=0&channel=stale.dot`)).status,
    ).toBe(409);
    // A host that stamps no identity at all is refused too: absent is not trusted.
    await stream({ channelId: "bare.dot", dir: "out", frame });
    expect(
      (await fetch(`${base}/frame?id=p:1&i=0&channel=bare.dot`)).status,
    ).toBe(409);
    // Payload-blind grouping is unaffected: both ops are recorded regardless.
    const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
    expect(traces.length).toBe(2);
  } finally {
    server.stop();
  }
});

test("isLoopbackDebugHost is an exact allowlist (drives the Host-header guard)", () => {
  expect(isLoopbackDebugHost("127.0.0.1")).toBe(true);
  expect(isLoopbackDebugHost("localhost")).toBe(true);
  expect(isLoopbackDebugHost("::1")).toBe(true);
  // Everything else is non-loopback. A fuzzy match that read any of these as
  // loopback would let a rebound page past the DNS-rebinding Host guard.
  for (const host of [
    "0.0.0.0",
    "127.0.0.1.evil.com",
    "127.0.0.2",
    "[::1]",
    "LOCALHOST",
    "example.com",
  ]) {
    expect(isLoopbackDebugHost(host)).toBe(false);
  }
});

test("/frame rejects out-of-range indices (negative and huge) with 404", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);
    // Integer but out of range ⇒ 404 (no such frame); non-integer ⇒ 400.
    expect((await fetch(`${base}/frame?id=p:1&i=-1`)).status).toBe(404);
    expect((await fetch(`${base}/frame?id=p:1&i=99999`)).status).toBe(404);
    expect((await fetch(`${base}/frame?id=p:1&i=1.5`)).status).toBe(400);
  } finally {
    server.stop();
  }
});

test("a default server decodes every frame, including formerly-sensitive ones", async () => {
  // Dev-only tool: decode is on by default, so a signing frame decodes.
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    expect(server.decodeValues).toBe(true);
    await streamFrame(base, server.port, signFrame("p:sign"));
    const detail = await (await fetch(`${base}/frame?id=p:sign&i=0`)).json();
    expect(detail.kind).toBe("decoded");
    expect(JSON.stringify(detail.value)).toContain("alice.dot");
    // No sensitive/redacted machinery: `?reveal=0` is just an unknown param,
    // ignored, and the frame still decodes.
    const still = await (
      await fetch(`${base}/frame?id=p:sign&i=0&reveal=0`)
    ).json();
    expect(still.kind).toBe("decoded");
  } finally {
    server.stop();
  }
});

test("a page with a non-loopback Host header is refused (DNS-rebinding guard)", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // A rebound evil.com -> 127.0.0.1 page's same-origin fetch still carries its
    // own Host; a non-loopback (non-bind) Host must be refused with a 403.
    const res = await fetch(`${base}/traces`, {
      headers: { host: "evil.com" },
    });
    expect(res.status).toBe(403);
    // A loopback Host is fine.
    const ok = await fetch(`${base}/traces`, {
      headers: { host: `127.0.0.1:${server.port}` },
    });
    expect(ok.status).toBe(200);
  } finally {
    server.stop();
  }
});

test("groups by (channel, requestId) — two hosts minting the same id do not merge", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // Per-transport counters mean both hosts mint requestId "p:1" for different
    // ops. They must NOT collapse into one trace.
    // Distinct byte lengths so the per-channel drill-down is distinguishable.
    const a = encodeFrame("p:1", W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([1]));
    const b = encodeFrame(
      "p:1",
      W.CHAIN_GET_HEAD_HEADER.request,
      new Uint8Array([2, 2, 2]),
    );
    const send = async (frame: string, channelId: string) => {
      const ws = new WebSocket(`ws://localhost:${server.port}`);
      await new Promise<void>((resolve, reject) => {
        ws.onopen = () => resolve();
        ws.onerror = () => reject(new Error("ws failed to open"));
      });
      ws.send(
        JSON.stringify({
          channelId,
          dir: "out",
          frame,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
        }),
      );
      await new Promise((r) => setTimeout(r, 40));
      ws.close();
    };
    await send(a, "hostA.dot");
    await send(b, "hostB.dot");

    interface Ch {
      channelId: string;
      requestId: string;
      frames: TraceFrameView[];
    }
    let traces: Ch[] = [];
    for (let i = 0; i < 50; i++) {
      traces = (await (await fetch(`${base}/traces`)).json()) as Ch[];
      if (traces.length >= 2) break;
      await new Promise((r) => setTimeout(r, 20));
    }
    // Two separate traces: same requestId, distinct channels, distinct frames.
    expect(traces).toHaveLength(2);
    const byChannel = new Map(traces.map((t) => [t.channelId, t]));
    expect(byChannel.get("hostA.dot")?.requestId).toBe("p:1");
    expect(byChannel.get("hostB.dot")?.requestId).toBe("p:1");
    expect(byChannel.get("hostA.dot")?.frames[0].frameId).toBe(
      W.ACCOUNT_GET_ACCOUNT.request,
    );
    expect(byChannel.get("hostB.dot")?.frames[0].frameId).toBe(
      W.CHAIN_GET_HEAD_HEADER.request,
    );

    // /frame disambiguates by channel: same id "p:1" resolves to the right
    // host's frame (distinct byte lengths prove it's not the other channel's).
    const detailA = await (
      await fetch(`${base}/frame?id=p:1&i=0&channel=hostA.dot`)
    ).json();
    const detailB = await (
      await fetch(`${base}/frame?id=p:1&i=0&channel=hostB.dot`)
    ).json();
    expect(detailA.byteLength).toBe(1);
    expect(detailB.byteLength).toBe(3);
    expect(detailA).not.toEqual(detailB);
  } finally {
    server.stop();
  }
});
