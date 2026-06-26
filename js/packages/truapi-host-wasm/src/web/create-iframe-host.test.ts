import { afterEach, beforeEach, describe, expect, it, mock } from "bun:test";

import { createIframeHost } from "./index.js";

// Verify that `createIframeHost` hands a MessagePort back through `onPort`,
// constructs an iframe with the expected attributes, and posts the
// `truapi-init` handshake after the iframe reports readiness.

function setupFakeDom() {
    // Track listeners on the synthetic `window` and the iframe so the
    // test can simulate the iframe `load` event after construction.
    const iframeListeners = new Map<string, (event: unknown) => void>();
    const windowListeners = new Map<string, (event: unknown) => void>();
    const windowRemove = mock((_name: string, _fn: unknown) => {});
    const contentPostMessage = mock((_body: unknown, _origin: string) => {});

    const contentWindow = {
        postMessage: contentPostMessage,
    };

    const iframe = {
        style: {} as Record<string, unknown>,
        setAttribute: mock((_name: string, _value: string) => {}),
        addEventListener: (name: string, fn: (event: unknown) => void) => {
            iframeListeners.set(name, fn);
        },
        removeEventListener: () => {},
        remove: mock(() => {}),
        referrerPolicy: "",
        credentialless: false,
        allow: "",
        src: "",
        contentWindow,
    };

    const container = {
        appendChild: mock((_child: unknown) => {}),
    };

    // Spy on both MessageChannel ports so dispose() teardown is observable.
    const port1 = { postMessage: mock(() => {}), close: mock(() => {}) };
    const port2 = { postMessage: mock(() => {}), close: mock(() => {}) };
    globalThis.MessageChannel = class {
        port1 = port1;
        port2 = port2;
    } as unknown as typeof MessageChannel;

    globalThis.document = {
        createElement: (tag: string) => {
            expect(tag).toBe("iframe");
            return iframe as unknown as HTMLIFrameElement;
        },
    } as unknown as Document;
    globalThis.window = {
        location: { href: "http://localhost:5174/" },
        addEventListener: (name: string, fn: (event: unknown) => void) => {
            windowListeners.set(name, fn);
        },
        removeEventListener: windowRemove,
    } as unknown as Window & typeof globalThis;

    return {
        iframe,
        container,
        contentPostMessage,
        contentWindow,
        iframeListeners,
        windowListeners,
        windowRemove,
        port1,
        port2,
    };
}

function teardownFakeDom() {
    delete (globalThis as { document?: unknown }).document;
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { MessageChannel?: unknown }).MessageChannel;
}

describe("createIframeHost", () => {
    let dom: ReturnType<typeof setupFakeDom>;

    beforeEach(() => {
        dom = setupFakeDom();
    });

    afterEach(() => {
        teardownFakeDom();
    });

    it("hands back a MessagePort and configures the iframe", () => {
        const { iframe, container, iframeListeners, windowRemove, port1, port2 } = dom;

        let receivedPort: MessagePort | null = null;
        const host = createIframeHost({
            iframeUrl: "http://localhost:5174/",
            container: container as unknown as HTMLElement,
            allow: "camera; cross-origin-isolated",
            onPort: (port) => {
                receivedPort = port;
            },
        });

        expect(receivedPort).toBeTruthy();
        expect(typeof receivedPort!.postMessage).toBe("function");
        expect(container.appendChild.mock.calls.length).toBe(1);
        expect(host.iframe).toBe(iframe as unknown as HTMLIFrameElement);
        expect(iframe.credentialless).toBe(true);
        expect(iframe.allow).toBe("camera; cross-origin-isolated");
        expect(iframe.src).toBe("http://localhost:5174/");
        // port transfer waits for explicit iframe readiness
        expect(iframeListeners.has("load")).toBe(false);

        host.dispose();
        expect(iframe.remove.mock.calls.length).toBe(1);
        // dispose removes the window message listener
        expect(windowRemove.mock.calls.length).toBe(1);
        expect(windowRemove.mock.calls[0][0]).toBe("message");
        // host + product ports closed on dispose
        expect(port1.close.mock.calls.length).toBe(1);
        expect(port2.close.mock.calls.length).toBe(1);
    });

    it("sends truapi-init on a same-origin product-ready message", () => {
        const { contentPostMessage, windowListeners, contentWindow } = dom;

        createIframeHost({
            iframeUrl: "http://localhost:5174/",
            container: { appendChild: () => {} } as unknown as HTMLElement,
            onPort: () => {},
        });

        const onMessage = windowListeners.get("message");
        expect(onMessage).toBeTruthy();

        // Wrong source is dropped.
        onMessage!({
            source: { other: true },
            origin: "http://localhost:5174",
            data: { type: "truapi-ready" },
        });
        expect(contentPostMessage.mock.calls.length).toBe(0);

        // Wrong origin is dropped.
        onMessage!({
            source: contentWindow,
            origin: "http://evil.example",
            data: { type: "truapi-ready" },
        });
        expect(contentPostMessage.mock.calls.length).toBe(0);

        // Correct source + origin triggers the init handshake.
        onMessage!({
            source: contentWindow,
            origin: "http://localhost:5174",
            data: { type: "truapi-ready" },
        });
        expect(contentPostMessage.mock.calls.length).toBe(1);
        const [body, origin] = contentPostMessage.mock.calls[0];
        expect(body).toEqual({ type: "truapi-init" });
        expect(origin).toBe("*");

        // The handshake is idempotent across repeated ready events too.
        onMessage!({
            source: contentWindow,
            origin: "http://localhost:5174",
            data: { type: "truapi-ready" },
        });
        expect(contentPostMessage.mock.calls.length).toBe(1);
    });

    it("accepts product-ready from a credentialless opaque origin", () => {
        const { contentPostMessage, windowListeners, contentWindow } = dom;

        createIframeHost({
            iframeUrl: "http://localhost:5174/",
            container: { appendChild: () => {} } as unknown as HTMLElement,
            onPort: () => {},
        });

        const onMessage = windowListeners.get("message");
        expect(onMessage).toBeTruthy();

        onMessage!({
            source: contentWindow,
            origin: "null",
            data: { type: "truapi-ready" },
        });
        expect(contentPostMessage.mock.calls.length).toBe(1);
        const [, origin] = contentPostMessage.mock.calls[0];
        expect(origin).toBe("*");
    });

    it("rejects a mismatched allowedOrigin", () => {
        expect(() =>
            createIframeHost({
                iframeUrl: "http://localhost:5174/",
                container: { appendChild: () => {} } as unknown as HTMLElement,
                onPort: () => {},
                allowedOrigin: "http://localhost:9999",
            }),
        ).toThrow(/origin policy mismatch/);
    });

    it("rejects non-http(s) iframe URLs", () => {
        expect(() =>
            createIframeHost({
                iframeUrl: "file:///etc/passwd",
                container: { appendChild: () => {} } as unknown as HTMLElement,
                onPort: () => {},
            }),
        ).toThrow(/only allows http\(s\)/);
    });
});
