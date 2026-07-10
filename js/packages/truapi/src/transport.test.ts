import { afterEach, beforeEach, describe, expect, it } from "bun:test";

import { createIframeProvider, createMessagePortProvider } from "./transport.js";

/**
 * Install a minimal stub for the global `window` used by `createIframeProvider`.
 * Returns a dispatch helper and a snapshot of the registered message listeners
 * so individual tests can inspect cleanup.
 */
function installFakeWindow() {
    const listeners = new Set<(event: unknown) => void>();
    const prior = globalThis.window;
    globalThis.window = {
        addEventListener(name: string, cb: (event: unknown) => void) {
            if (name === "message") listeners.add(cb);
        },
        removeEventListener(name: string, cb: (event: unknown) => void) {
            if (name === "message") listeners.delete(cb);
        },
    } as unknown as Window & typeof globalThis;
    return {
        listeners,
        dispatch(event: unknown) {
            for (const cb of [...listeners]) cb(event);
        },
        restore() {
            if (prior === undefined) {
                delete (globalThis as { window?: unknown }).window;
            } else {
                globalThis.window = prior;
            }
        },
    };
}

describe("createIframeProvider", () => {
    let win: ReturnType<typeof installFakeWindow>;

    beforeEach(() => {
        win = installFakeWindow();
    });

    afterEach(() => {
        win.restore();
    });

    it("filters by source/origin/type and pins the outbound origin", () => {
        const sent: { msg: Uint8Array; origin: string }[] = [];
        const target = {
            postMessage(msg: Uint8Array, origin: string) {
                sent.push({ msg, origin });
            },
        } as unknown as Window;
        const provider = createIframeProvider({
            target,
            hostOrigin: "https://host.example",
        });

        const received: Uint8Array[] = [];
        provider.subscribe((m) => received.push(m));

        win.dispatch({
            source: target,
            origin: "https://host.example",
            data: new Uint8Array([1, 2, 3]),
        });
        expect([...received[0]]).toEqual([1, 2, 3]);

        // Bad source, bad origin, and non-bytes payloads are all dropped.
        win.dispatch({ source: {}, origin: "https://host.example", data: new Uint8Array([9]) });
        win.dispatch({
            source: target,
            origin: "https://attacker.example",
            data: new Uint8Array([9]),
        });
        win.dispatch({ source: target, origin: "https://host.example", data: "not bytes" });
        expect(received).toHaveLength(1);

        provider.postMessage(new Uint8Array([7]));
        expect(sent).toHaveLength(1);
        expect(sent[0].origin).toBe("https://host.example");
        expect([...sent[0].msg]).toEqual([7]);

        provider.dispose();
    });

    it("disposes idempotently and reports the close error to late subscribers", () => {
        const provider = createIframeProvider({
            target: { postMessage() {} } as unknown as Window,
            hostOrigin: "https://host.example",
        });

        let closeError: unknown = null;
        provider.subscribeClose?.((e) => (closeError = e));

        expect(win.listeners.size).toBeGreaterThan(0);
        provider.dispose();
        expect(closeError).toBeInstanceOf(Error);
        expect(win.listeners.size).toBe(0);

        // Idempotent: a second dispose is a no-op, and sends after close throw.
        provider.dispose();
        expect(() => provider.postMessage(new Uint8Array([1]))).toThrow();

        // Post-close subscribeClose fires immediately with the stored error.
        let late: unknown = null;
        provider.subscribeClose?.((e) => (late = e));
        expect(late).toBeInstanceOf(Error);
    });
});

describe("createMessagePortProvider", () => {
    it("queues sends, round-trips inbound frames, and errors after dispose", async () => {
        const { port1, port2 } = new MessageChannel();
        const provider = createMessagePortProvider(port1);

        provider.postMessage(new Uint8Array([42]));

        const drained = await new Promise<Uint8Array>((resolve) => {
            port2.onmessage = (e) => resolve(e.data);
            port2.start();
        });
        expect([...drained]).toEqual([42]);

        const inboundOnce = new Promise<Uint8Array>((resolve) => {
            const unsubscribe = provider.subscribe((m) => {
                unsubscribe();
                resolve(m);
            });
        });
        port2.postMessage(new Uint8Array([55]));
        expect([...(await inboundOnce)]).toEqual([55]);

        provider.dispose();
        let lateClose: unknown = null;
        provider.subscribeClose?.((e) => (lateClose = e));
        expect(lateClose).toBeInstanceOf(Error);
        expect(() => provider.postMessage(new Uint8Array([1]))).toThrow();

        // Free the receiver port so the runtime exits cleanly.
        port2.close();
    });
});
