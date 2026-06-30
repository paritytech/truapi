import { describe, expect, it } from "bun:test";
import { ok } from "neverthrow";

import type { CoreStorageKey } from "../generated/host-callbacks.js";
import { createMockHost, mockRuntimeConfig } from "./create-mock-host.js";
import { createWebWorkerProvider } from "./index.js";

describe("createMockHost callbacks", () => {
    it("product storage round-trips and is namespaced from core", async () => {
        const { callbacks } = createMockHost();
        await callbacks.write("k", new Uint8Array([1, 2, 3]));
        expect(await callbacks.read("k")).toEqual(new Uint8Array([1, 2, 3]));
        // A product key never collides with a core slot.
        expect(await callbacks.readCoreStorage({ tag: "AuthSession" })).toBeUndefined();
        await callbacks.clear("k");
        expect(await callbacks.read("k")).toBeUndefined();
    });

    it("core storage round-trips per slot", async () => {
        const { callbacks } = createMockHost();
        const key: CoreStorageKey = {
            tag: "PermissionAuthorization",
            value: { storageKey: "cam" },
        };
        await callbacks.writeCoreStorage(key, new Uint8Array([9]));
        expect(await callbacks.readCoreStorage(key)).toEqual(new Uint8Array([9]));
        await callbacks.clearCoreStorage(key);
        expect(await callbacks.readCoreStorage(key)).toBeUndefined();
    });

    it("permissions follow per-capability policy", async () => {
        const { callbacks } = createMockHost({
            devicePermissions: "allow-all",
            remotePermissions: "deny-all",
        });
        expect((await callbacks.devicePermission("Notifications")).granted).toBe(true);
        expect((await callbacks.remotePermission({ permission: { tag: "WebRtc" } })).granted).toBe(
            false,
        );
    });

    it("feature support and theme reflect config", async () => {
        const { callbacks } = createMockHost({ featureSupported: false, theme: "Light" });
        expect(
            (
                await callbacks.featureSupported({
                    tag: "Chain",
                    value: { genesisHash: "0x00" },
                })
            ).supported,
        ).toBe(false);
        const theme = await callbacks.subscribeTheme()[Symbol.asyncIterator]().next();
        expect(theme.value).toEqual(ok("Light"));
    });

    it("records navigations and assigns monotonic notification ids", async () => {
        const host = createMockHost();
        await host.callbacks.navigateTo("https://a");
        await host.callbacks.navigateTo("https://b");
        expect(host.navigations()).toEqual(["https://a", "https://b"]);

        const first = await host.callbacks.pushNotification({ text: "one" });
        const second = await host.callbacks.pushNotification({ text: "two" });
        expect([first.id, second.id]).toEqual([0, 1]);
        expect(host.pushedNotifications().length).toBe(2);
    });

    it("confirms per config and records chain sends", async () => {
        const denied = createMockHost({ confirmUserActions: false });
        expect(
            await denied.callbacks.confirmUserAction?.({
                tag: "ResourceAllocation",
                value: { resources: [] },
            }),
        ).toBe(false);

        const host = createMockHost();
        const conn = await host.callbacks.connect(new Uint8Array(32));
        conn.send("rpc-1");
        expect(host.sentRpc()).toEqual(["rpc-1"]);
    });

    it("replays scripted chain frames", async () => {
        const host = createMockHost({ chainResponses: ["f1", "f2"] });
        const conn = await host.callbacks.connect(new Uint8Array(32));
        const frames: string[] = [];
        for await (const frame of conn.responses()) {
            frames.push(frame);
        }
        expect(frames).toEqual(["f1", "f2"]);
    });

    it("preimage submit then lookup round-trips", async () => {
        const { callbacks } = createMockHost();
        const key = await callbacks.submitPreimage?.(new Uint8Array([4, 5, 6]));
        expect(key).toBeDefined();
        const found = await callbacks.lookupPreimage(key!)[Symbol.asyncIterator]().next();
        expect(found.value).toEqual(ok(new Uint8Array([4, 5, 6])));
    });
});

/** Minimal `Worker` stand-in: records posted messages and lets the test drive
 *  the `message` event by hand, so the provider initializes without real WASM. */
class FakeWorker {
    listeners = new Map<string, Set<(event: unknown) => void>>();
    messages: Record<string, unknown>[] = [];

    addEventListener(name: string, fn: (event: unknown) => void) {
        const set = this.listeners.get(name) ?? new Set();
        set.add(fn);
        this.listeners.set(name, set);
    }

    removeEventListener(name: string, fn: (event: unknown) => void) {
        this.listeners.get(name)?.delete(fn);
    }

    postMessage(message: Record<string, unknown>) {
        this.messages.push(message);
    }

    terminate() {}

    emit(message: Record<string, unknown>) {
        for (const listener of this.listeners.get("message") ?? []) {
            listener({ data: message });
        }
    }
}

describe("createMockHost with createWebWorkerProvider", () => {
    it("initializes a worker provider with the mock callbacks (no real WASM)", async () => {
        const worker = new FakeWorker();
        const host = createMockHost();
        const providerPromise = createWebWorkerProvider(
            worker as unknown as Worker,
            host.callbacks,
            { runtimeConfig: mockRuntimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });

        const provider = await providerPromise;
        expect(provider).toBeDefined();
        const init = worker.messages.find((message) => message.kind === "init");
        expect(init).toBeDefined();
    });
});
