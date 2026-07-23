import { afterEach, describe, expect, it, mock } from "bun:test";

import { encodeWireMessage } from "./transport.js";

let importCounter = 0;

async function importSandbox(): Promise<typeof import("./sandbox.js")> {
    importCounter += 1;
    return import(`./sandbox.ts?test=${importCounter}`);
}

type MessageListener = (event: MessageEvent) => void;

function installFakeIframeWindow(options: { referrer?: string; ancestorOrigins?: string[] }) {
    const listeners = new Set<MessageListener>();
    const priorWindow = globalThis.window;
    const priorDocument = globalThis.document;
    const parentPostMessage = mock((_message: unknown, _origin: string) => {});
    const parent = {
        postMessage: parentPostMessage,
    } as unknown as Window;
    const win = {
        parent,
        top: {} as Window,
        location: {
            ancestorOrigins: options.ancestorOrigins,
        },
        addEventListener(name: string, callback: EventListener) {
            if (name === "message") listeners.add(callback as MessageListener);
        },
        removeEventListener(name: string, callback: EventListener) {
            if (name === "message") listeners.delete(callback as MessageListener);
        },
    } as unknown as Window & typeof globalThis;

    globalThis.window = win;
    globalThis.document = {
        referrer: options.referrer ?? "",
    } as Document;

    return {
        listeners,
        parent,
        parentPostMessage,
        win,
        dispatch(event: { source: unknown; origin: string; data: unknown; ports?: MessagePort[] }) {
            for (const listener of [...listeners]) {
                listener({ ports: [], ...event } as MessageEvent);
            }
        },
        restore() {
            if (priorWindow === undefined) {
                delete (globalThis as { window?: unknown }).window;
            } else {
                globalThis.window = priorWindow;
            }
            if (priorDocument === undefined) {
                delete (globalThis as { document?: unknown }).document;
            } else {
                globalThis.document = priorDocument;
            }
        },
    };
}

let currentWindow: ReturnType<typeof installFakeIframeWindow> | null = null;
const openPorts: MessagePort[] = [];

function trackChannel(): MessageChannel {
    const channel = new MessageChannel();
    openPorts.push(channel.port1, channel.port2);
    return channel;
}

afterEach(() => {
    for (const port of openPorts.splice(0)) {
        port.close();
    }
    currentWindow?.restore();
    currentWindow = null;
});

describe("sandbox iframe MessagePort handshake", () => {
    it("posts ready to the resolved host origin and rejects non-parent or mismatched init messages", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        expect(currentWindow.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "https://host.example"],
        ]);

        const wrongSource = trackChannel();
        currentWindow.dispatch({
            source: {},
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [wrongSource.port1],
        });
        const wrongOrigin = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://attacker.example",
            data: { type: "truapi-init" },
            ports: [wrongOrigin.port1],
        });
        const opaqueOrigin = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "null",
            data: { type: "truapi-init" },
            ports: [opaqueOrigin.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBeUndefined();
        expect(currentWindow.listeners.size).toBe(1);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBe(accepted.port1);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it('treats a masked "null" ancestor origin as hidden and pings with the wildcard', async () => {
        // Firefox implements location.ancestorOrigins but serializes cross-origin
        // ancestors as "null", which is not a valid postMessage targetOrigin.
        currentWindow = installFakeIframeWindow({ ancestorOrigins: ["null"] });
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        expect(currentWindow.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "*"],
        ]);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBe(accepted.port1);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it("uses a data-free wildcard ready ping only when the host origin is hidden", async () => {
        currentWindow = installFakeIframeWindow({});
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        expect(currentWindow.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "*"],
        ]);

        const wrongSource = trackChannel();
        currentWindow.dispatch({
            source: {},
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [wrongSource.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBeUndefined();

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBe(accepted.port1);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it("reports connecting until the MessagePort handover completes", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connecting"]);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        expect(statuses).toEqual(["connecting", "connected"]);
    });

    it("reports connecting until the first legacy frame pins the transport", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://legacy-host.example/product",
        });
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connecting"]);

        const probe = encodeWireMessage({
            requestId: "legacy-probe",
            payload: { id: 255, value: new Uint8Array() },
        });
        expect(probe.isOk()).toBe(true);
        if (probe.isErr()) throw probe.error;
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://legacy-host.example",
            data: probe.value,
        });
        expect(statuses).toEqual(["connecting", "connected"]);
    });

    it("reports connected immediately when the host port is already injected", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const channel = trackChannel();
        currentWindow.win.__HOST_API_PORT__ = channel.port1;
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connected"]);
    });

    it("falls back to legacy window frames and pins their parent origin", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://legacy-host.example/product",
        });
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync();
        expect(client).not.toBeNull();

        const probe = encodeWireMessage({
            requestId: "legacy-probe",
            payload: { id: 255, value: new Uint8Array() },
        });
        expect(probe.isOk()).toBe(true);
        if (probe.isErr()) throw probe.error;
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://legacy-host.example",
            data: probe.value,
        });

        void client?.system.handshake();
        expect(currentWindow.parentPostMessage.mock.calls).toHaveLength(2);
        expect(currentWindow.parentPostMessage.mock.calls[1][0]).toBeInstanceOf(Uint8Array);
        expect(currentWindow.parentPostMessage.mock.calls[1][1]).toBe(
            "https://legacy-host.example",
        );
    });
});
