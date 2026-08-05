import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { encodeWireMessage, VersionedHostAccountGetRequest } from "@parity/truapi";
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

/** A real, decodable account-get request wire message (non-sensitive). */
function accountGetRequestBytes(): Uint8Array {
  const value = VersionedHostAccountGetRequest.enc({
    tag: "V1",
    value: {
      productAccountId: {
        dotNsIdentifier: "alice.dot",
        derivationIndex: { tag: "Left", value: 0 },
      },
    },
  });
  const r = encodeWireMessage({
    requestId: "p:1",
    payload: { id: W.ACCOUNT_GET_ACCOUNT.request, value },
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

  test("feeds frames in-process and decodes by default", () => {
    const dbg = createInAppDebugger(); // decode ON by default (dev-only tool)

    // Two frames of one op, fed exactly as dotli's tap would (raw SCALE bytes).
    // The request leg carries a real, decodable account-get payload.
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes());
    dbg.handleFrame("shop.dot", "in", frameBytes(W.ACCOUNT_GET_ACCOUNT.response));

    expect(dbg.session.traceEngine.traces()).toHaveLength(1);
    expect(dbg.session.decodeValues).toBe(true); // decodes by default

    // The drill-down surfaces the decoded value.
    const detail = dbg.session.frameDetail("p:1", 0, "shop.dot");
    expect(detail?.kind).toBe("decoded");

    const el = fakeEl();
    const dispose = dbg.mount(el as unknown as HTMLElement);
    const list = el.children[1]; // [style, list]
    // Rendered by the shared renderer — the method resolved via the wire table.
    expect(list.innerHTML).toContain("account.getAccount");
    dispose();
    expect(list.children).toHaveLength(0);
  });

  test("a formerly-sensitive op is no longer special-cased (never redacted)", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", frameBytes(W.SIGNING_SIGN_RAW.request, [1, 2]));
    dbg.handleFrame("shop.dot", "in", frameBytes(W.SIGNING_SIGN_RAW.response));
    const view = dbg.session.traceEngine.traces()[0];
    expect(view).toBeDefined();
    // No denylist: the drill-down either decodes or falls back to bytes, but
    // never returns the old "redacted" state.
    const detail = dbg.session.frameDetail("p:1", 0, "shop.dot");
    expect(["decoded", "bytes"]).toContain(detail?.kind);
    expect(detail?.kind).not.toBe("redacted");
  });

  test("decodeValues:false keeps the mount payload-blind (bytes only)", () => {
    const dbg = createInAppDebugger({ decodeValues: false });
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes());
    expect(dbg.session.decodeValues).toBe(false);
    expect(dbg.session.frameDetail("p:1", 0, "shop.dot")?.kind).toBe("bytes");
  });
});
