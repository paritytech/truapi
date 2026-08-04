import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { encodeWireMessage } from "@parity/truapi";
import * as W from "@parity/truapi/wire-table";

import { createInAppDebugger } from "./in-app.js";

// A minimal element stand-in — the mount only needs createElement, append,
// textContent/className/innerHTML, and remove(). No real DOM needed.
interface FakeEl {
  textContent: string;
  className: string;
  innerHTML: string;
  children: FakeEl[];
  append(...nodes: FakeEl[]): void;
  remove(): void;
}
function fakeEl(): FakeEl {
  return {
    textContent: "",
    className: "",
    innerHTML: "",
    children: [],
    append(...nodes) {
      this.children.push(...nodes);
    },
    remove() {},
  };
}

function frameBytes(id: number, value: number[] = [0]): Uint8Array {
  const r = encodeWireMessage({
    requestId: "p:1",
    payload: { id, value: new Uint8Array(value) },
  });
  if (r.isErr()) throw r.error;
  return r.value;
}

describe("createInAppDebugger", () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- shim a DOM
  const g = globalThis as any;
  const original = g.document;
  beforeAll(() => {
    g.document = { createElement: (): FakeEl => fakeEl() };
  });
  afterAll(() => {
    g.document = original;
  });

  test("feeds frames in-process and mounts a payload-blind panel", () => {
    const dbg = createInAppDebugger(); // decode OFF by default

    // Two frames of one op, fed exactly as dotli's tap would (raw SCALE bytes).
    dbg.handleFrame("shop.dot", "out", frameBytes(W.ACCOUNT_GET_ACCOUNT.request));
    dbg.handleFrame("shop.dot", "in", frameBytes(W.ACCOUNT_GET_ACCOUNT.response));

    expect(dbg.session.traceEngine.traces()).toHaveLength(1);
    expect(dbg.session.decodeValues).toBe(false); // payload-blind by default
    expect(dbg.session.revealSensitive).toBe(false);

    const el = fakeEl();
    const dispose = dbg.mount(el as unknown as HTMLElement);
    const list = el.children[1]; // [style, list]
    // Rendered by the shared renderer — the method resolved via the wire table.
    expect(list.innerHTML).toContain("account.getAccount");
    dispose();
    expect(list.children).toHaveLength(0);
  });

  test("a sensitive op stays redacted with decode off", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", frameBytes(W.SIGNING_SIGN_RAW.request, [1, 2]));
    dbg.handleFrame("shop.dot", "in", frameBytes(W.SIGNING_SIGN_RAW.response));
    const view = dbg.session.traceEngine.traces()[0];
    expect(view).toBeDefined();
    // The signing op is on the type-driven denylist, so the session flags it.
    expect(
      dbg.session.frameDetail("p:1", 0, "shop.dot")?.kind,
    ).not.toBe("decoded");
  });
});
