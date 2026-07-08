import { describe, expect, it } from "bun:test";
import { ok } from "neverthrow";

import { HostPushNotificationRequest, HostPushNotificationResponse } from "@parity/truapi";
import type { GenericError, Result, ThemeVariant } from "@parity/truapi";

import { createWasmRawCallbacks } from "../generated/host-callbacks-adapter.js";
import { AuthState, CoreStorageKey } from "../generated/host-callbacks.js";
import type { AuthState as AuthStateValue, PreimageHost } from "../generated/host-callbacks.js";
import type { ProductRuntimeConfig, TrUApiProductProvider } from "../runtime.js";
import { makeHostCallbacks, settle } from "../test-support.js";
import { createWebWorkerPairingHostRuntime } from "./index.js";
import type { CreateWebWorkerPairingHostRuntimeOptions } from "./index.js";

type WorkerMessage = Record<string, unknown>;

/** Minimal `Worker` stand-in that records posted messages and lets a test
 *  drive the `message`/`error`/`messageerror` events by hand. */
class FakeWorker {
    listeners = new Map<string, Set<(event: unknown) => void>>();
    messages: WorkerMessage[] = [];
    terminated = false;

    addEventListener(name: string, fn: (event: unknown) => void) {
        const listeners = this.listeners.get(name) ?? new Set();
        listeners.add(fn);
        this.listeners.set(name, listeners);
    }

    removeEventListener(name: string, fn: (event: unknown) => void) {
        this.listeners.get(name)?.delete(fn);
    }

    postMessage(message: WorkerMessage) {
        this.messages.push(message);
    }

    terminate() {
        this.terminated = true;
    }

    emit(message: WorkerMessage) {
        for (const listener of this.listeners.get("message") ?? []) {
            listener({ data: message });
        }
    }

    emitError(message: string) {
        for (const listener of this.listeners.get("error") ?? []) {
            listener({ message });
        }
    }

    emitMessageError() {
        for (const listener of this.listeners.get("messageerror") ?? []) {
            listener({ data: null });
        }
    }
}

/** Coerce the `FakeWorker` to the `Worker` shape the provider expects. */
function asWorker(worker: FakeWorker): Worker {
    return worker as unknown as Worker;
}

function runtimeConfig(overrides: Partial<ProductRuntimeConfig> = {}): ProductRuntimeConfig {
    return {
        productId: "dotli.dot",
        host: {
            name: "Polkadot Web",
            icon: "https://dot.li/dotli.png",
            version: "0.5.0",
        },
        platform: {
            type: "node",
            version: process.versions.node,
        },
        people: {
            genesisHash: "0xa22a2424d2cbf561eaecf7da8b1b548fa9d1939f60265e942b1049616a012f71",
        },
        pairing: {
            deeplinkScheme: "polkadotapp",
        },
        ...overrides,
    };
}

function hostConfigFromRuntimeConfig(
    config: ProductRuntimeConfig,
): CreateWebWorkerPairingHostRuntimeOptions["hostConfig"] {
    const { productId: _productId, ...hostConfig } = config;
    return hostConfig;
}

function lastMessageOfKind(worker: FakeWorker, kind: string): WorkerMessage {
    const message = [...worker.messages].reverse().find((m) => m.kind === kind);
    expect(message).toBeDefined();
    return message!;
}

async function finishProviderReady(
    worker: FakeWorker,
    providerPromise: Promise<TrUApiProductProvider>,
) {
    await settle();
    const createCore = lastMessageOfKind(worker, "createCore");
    worker.emit({ kind: "coreReady", coreId: createCore.coreId });
    return providerPromise;
}

type ReadyOptions = Partial<Omit<CreateWebWorkerPairingHostRuntimeOptions, "hostConfig">> & {
    createWebWorkerPairingHostRuntime?: typeof createWebWorkerPairingHostRuntime;
    runtimeConfig?: ProductRuntimeConfig;
};

async function createProviderFromRuntime(
    worker: Worker,
    host: ReturnType<typeof makeHostCallbacks>,
    options: ReadyOptions,
): Promise<TrUApiProductProvider> {
    const {
        createWebWorkerPairingHostRuntime: createRuntime = createWebWorkerPairingHostRuntime,
        runtimeConfig: cfg = runtimeConfig(),
        ...runtimeOptions
    } = options;
    const runtime = await createRuntime(worker, host, {
        ...runtimeOptions,
        hostConfig: hostConfigFromRuntimeConfig(cfg),
    });
    const provider = await runtime.createProvider({ productId: cfg.productId });
    return {
        ...provider,
        dispose(): void {
            provider.dispose();
            runtime.dispose();
        },
    };
}

async function readyProvider(worker: FakeWorker, options: ReadyOptions = {}) {
    const providerPromise = createProviderFromRuntime(
        asWorker(worker),
        makeHostCallbacks(),
        options,
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    return finishProviderReady(worker, providerPromise);
}

/** Typed view of the dev console the worker runtime publishes on `globalThis`. */
type TruapiDevConsole = {
    setLogLevel(level: string): void;
    getLogLevel(): string | null;
};
const devGlobal = globalThis as typeof globalThis & {
    __truapi?: TruapiDevConsole;
};

describe("createWebWorkerPairingHostRuntime", () => {
    it("initializes the worker without a callback manifest", async () => {
        const worker = new FakeWorker();
        const config = runtimeConfig();
        const providerPromise = createProviderFromRuntime(asWorker(worker), makeHostCallbacks(), {
            logLevel: "debug",
            runtimeConfig: config,
        });

        worker.emit({ kind: "loaded" });
        expect(worker.messages.length).toBe(1);
        expect(worker.messages[0]).toEqual({
            kind: "init",
            logLevel: "debug",
            hostConfig: hostConfigFromRuntimeConfig(config),
        });

        worker.emit({ kind: "ready" });
        await settle();
        const createCore = lastMessageOfKind(worker, "createCore");
        expect(createCore).toEqual({
            kind: "createCore",
            coreId: 1,
            product: { productId: "dotli.dot" },
        });
        worker.emit({ kind: "coreReady", coreId: 1 });
        const provider = await providerPromise;
        expect(typeof provider.disconnectSession).toBe("function");

        provider.dispose();
    });

    it("creates multiple product cores on one worker runtime", async () => {
        const worker = new FakeWorker();
        const config = runtimeConfig();
        const runtimePromise = createWebWorkerPairingHostRuntime(
            asWorker(worker),
            makeHostCallbacks(),
            {
                hostConfig: hostConfigFromRuntimeConfig(config),
            },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const runtime = await runtimePromise;

        const firstPromise = runtime.createProvider({ productId: "first.dot" });
        const secondPromise = runtime.createProvider({ productId: "second.dot" });

        expect(worker.messages.at(-2)).toEqual({
            kind: "createCore",
            coreId: 1,
            product: { productId: "first.dot" },
        });
        expect(worker.messages.at(-1)).toEqual({
            kind: "createCore",
            coreId: 2,
            product: { productId: "second.dot" },
        });

        worker.emit({ kind: "coreReady", coreId: 1 });
        worker.emit({ kind: "coreReady", coreId: 2 });
        const first = await firstPromise;
        const second = await secondPromise;

        const firstFrames: Uint8Array[] = [];
        const secondFrames: Uint8Array[] = [];
        first.subscribe((frame) => firstFrames.push(frame));
        second.subscribe((frame) => secondFrames.push(frame));

        worker.emit({ kind: "frame", coreId: 2, bytes: new Uint8Array([2]) });
        worker.emit({ kind: "frame", coreId: 1, bytes: new Uint8Array([1]) });
        expect(firstFrames).toEqual([new Uint8Array([1])]);
        expect(secondFrames).toEqual([new Uint8Array([2])]);

        first.postMessage(new Uint8Array([9]));
        expect(worker.messages.at(-1)).toEqual({
            kind: "frame",
            coreId: 1,
            bytes: new Uint8Array([9]),
        });

        first.dispose();
        expect(worker.messages.at(-1)).toEqual({ kind: "disposeCore", coreId: 1 });

        worker.emit({ kind: "frame", coreId: 2, bytes: new Uint8Array([3]) });
        expect(firstFrames).toEqual([new Uint8Array([1])]);
        expect(secondFrames).toEqual([new Uint8Array([2]), new Uint8Array([3])]);

        runtime.dispose();
        expect(worker.messages.at(-1)).toEqual({ kind: "dispose" });
    });

    it("dev global setLogLevel updates every live worker provider", async () => {
        const previous = devGlobal.__truapi;
        delete devGlobal.__truapi;
        const firstWorker = new FakeWorker();
        const secondWorker = new FakeWorker();
        const first = await readyProvider(firstWorker);
        const second = await readyProvider(secondWorker);

        devGlobal.__truapi!.setLogLevel("debug");

        expect(firstWorker.messages.at(-1)).toEqual({
            kind: "setLogLevel",
            level: "debug",
        });
        expect(secondWorker.messages.at(-1)).toEqual({
            kind: "setLogLevel",
            level: "debug",
        });
        expect(devGlobal.__truapi!.getLogLevel()).toBe("debug");

        devGlobal.__truapi!.setLogLevel("off");
        first.dispose();
        second.dispose();
        if (previous === undefined) {
            delete devGlobal.__truapi;
        } else {
            devGlobal.__truapi = previous;
        }
    });

    it("dev global setLogLevel applies to providers created later", async () => {
        const previous = devGlobal.__truapi;
        delete devGlobal.__truapi;
        const moduleUrl = `./create-worker-host-runtime.js?dev-global-${Date.now()}`;
        const { createWebWorkerPairingHostRuntime: freshCreateWebWorkerPairingHostRuntime } =
            (await import(moduleUrl)) as typeof import("./create-worker-host-runtime.js");

        expect(typeof devGlobal.__truapi!.setLogLevel).toBe("function");
        devGlobal.__truapi!.setLogLevel("trace");

        const firstWorker = new FakeWorker();
        const first = await readyProvider(firstWorker, {
            createWebWorkerPairingHostRuntime: freshCreateWebWorkerPairingHostRuntime,
        });
        first.dispose();

        const secondWorker = new FakeWorker();
        const second = await readyProvider(secondWorker, {
            createWebWorkerPairingHostRuntime: freshCreateWebWorkerPairingHostRuntime,
        });

        expect(secondWorker.messages[0].kind).toBe("init");
        expect(secondWorker.messages[0].logLevel).toBe("trace");
        expect(
            secondWorker.messages.some((message) => {
                return message.kind === "setLogLevel" && message.level === "trace";
            }),
        ).toBe(true);

        second.dispose();
        devGlobal.__truapi!.setLogLevel("off");
        if (previous === undefined) {
            delete devGlobal.__truapi;
        } else {
            devGlobal.__truapi = previous;
        }
    });

    it("dev global setLogLevel persists the level to localStorage", async () => {
        const previousGlobal = devGlobal.__truapi;
        const previousStorage = globalThis.localStorage;
        delete devGlobal.__truapi;
        const store = new Map<string, string>();
        globalThis.localStorage = {
            getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
            setItem: (key: string, value: string) => store.set(key, String(value)),
        } as unknown as Storage;

        const worker = new FakeWorker();
        const provider = await readyProvider(worker);

        devGlobal.__truapi!.setLogLevel("debug");
        expect(store.get("truapi:logLevel")).toBe("debug");

        devGlobal.__truapi!.setLogLevel("off");
        expect(store.get("truapi:logLevel")).toBe("off");

        provider.dispose();
        globalThis.localStorage = previousStorage;
        if (previousGlobal === undefined) {
            delete devGlobal.__truapi;
        } else {
            devGlobal.__truapi = previousGlobal;
        }
    });

    it("resolves disconnect responses", async () => {
        const worker = new FakeWorker();
        const providerPromise = createProviderFromRuntime(asWorker(worker), makeHostCallbacks(), {
            runtimeConfig: runtimeConfig(),
        });
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        const disconnect = provider.disconnectSession();
        const msg = worker.messages.at(-1)!;
        expect(msg.kind).toBe("disconnectSession");
        expect(typeof msg.requestId).toBe("number");

        worker.emit({
            kind: "disconnectSessionResponse",
            requestId: msg.requestId,
            ok: true,
        });
        await disconnect;

        provider.dispose();
    });

    it("dispatches callback requests to host hooks", async () => {
        const worker = new FakeWorker();
        let clears = 0;
        const authSessionKey = CoreStorageKey.enc({ tag: "AuthSession" });
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({
                clearCoreStorage: async (key) => {
                    expect(key).toEqual({ tag: "AuthSession", value: undefined });
                    clears += 1;
                },
            }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "callbackRequest",
            requestId: 7,
            name: "clearCoreStorage",
            args: [authSessionKey],
        });
        await settle();

        expect(clears).toBe(1);
        expect(worker.messages.at(-1)).toEqual({
            kind: "callbackResponse",
            requestId: 7,
            ok: true,
            value: undefined,
        });

        provider.dispose();
    });

    it("reports unknown callback requests", async () => {
        const worker = new FakeWorker();
        const providerPromise = createProviderFromRuntime(asWorker(worker), makeHostCallbacks(), {
            runtimeConfig: runtimeConfig(),
        });
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "callbackRequest",
            requestId: 11,
            name: "someFutureCallback",
            args: [new Uint8Array([1, 2, 3])],
        });
        await settle();

        expect(worker.messages.at(-1)).toEqual({
            kind: "callbackResponse",
            requestId: 11,
            ok: false,
            error: "unknown callback: someFutureCallback",
        });

        provider.dispose();
    });

    it("forwards authStateChanged callback requests", async () => {
        const worker = new FakeWorker();
        const states: AuthStateValue[] = [];
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({
                authStateChanged: (state) => {
                    states.push(state);
                },
            }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);
        const publicKey = new Uint8Array(32);
        publicKey.set([1, 2]);

        worker.emit({
            kind: "callbackRequest",
            requestId: 3,
            name: "authStateChanged",
            args: [
                AuthState.enc({
                    tag: "Connected",
                    value: {
                        publicKey,
                        liteUsername: "alice",
                    },
                }),
            ],
        });
        await settle();

        expect(states).toEqual([
            {
                tag: "Connected",
                value: {
                    publicKey,
                    liteUsername: "alice",
                },
            },
        ]);
        expect(worker.messages.at(-1)).toEqual({
            kind: "callbackResponse",
            requestId: 3,
            ok: true,
            value: undefined,
        });

        provider.dispose();
    });

    it("revives Bulletin allowance signer handles for submitPreimage", async () => {
        const worker = new FakeWorker();
        const publicKey = new Uint8Array(32);
        publicKey.set([1, 2, 3]);
        const value = new Uint8Array([10, 11, 12]);
        const signingPayload = new Uint8Array([4, 5, 6]);
        const signature = new Uint8Array(64);
        signature.set([9, 8, 7]);
        const result = new Uint8Array([30, 31, 32]);
        const seen: {
            publicKey?: Uint8Array;
            signature?: Uint8Array;
            value?: Uint8Array;
        } = {};

        const submitPreimage: PreimageHost["submitPreimage"] = async (submittedValue, signer) => {
            seen.value = submittedValue;
            seen.publicKey = signer.publicKey;
            seen.signature = await signer.sign(signingPayload);
            return result;
        };
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({ submitPreimage }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "callbackRequest",
            requestId: 21,
            name: "submitPreimage",
            args: [value, { publicKey, signerId: 7 }],
        });
        await settle();

        const signRequest = lastMessageOfKind(worker, "signBulletinAllowance");
        expect(signRequest.kind).toBe("signBulletinAllowance");
        expect(signRequest.signerId).toBe(7);
        expect(signRequest.input).toEqual(signingPayload);
        expect(typeof signRequest.requestId).toBe("number");

        worker.emit({
            kind: "signBulletinAllowanceResponse",
            requestId: signRequest.requestId,
            ok: true,
            signature,
        });
        await settle();
        await settle();

        expect(seen).toEqual({
            value,
            publicKey,
            signature,
        });
        expect(worker.messages.at(-1)).toEqual({
            kind: "callbackResponse",
            requestId: 21,
            ok: true,
            value: result,
        });

        provider.dispose();
    });

    it("posts cancelPairing to the worker", async () => {
        const worker = new FakeWorker();
        const config = runtimeConfig();
        const runtimePromise = createWebWorkerPairingHostRuntime(
            asWorker(worker),
            makeHostCallbacks(),
            {
                hostConfig: hostConfigFromRuntimeConfig(config),
            },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const runtime = await runtimePromise;

        runtime.cancelPairing();

        expect(worker.messages.at(-1)).toEqual({ kind: "cancelPairing" });
        runtime.dispose();
    });

    it("posts notifySessionStoreChanged to the worker", async () => {
        const worker = new FakeWorker();
        const config = runtimeConfig();
        const runtimePromise = createWebWorkerPairingHostRuntime(
            asWorker(worker),
            makeHostCallbacks(),
            {
                hostConfig: hostConfigFromRuntimeConfig(config),
            },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const runtime = await runtimePromise;

        runtime.notifySessionStoreChanged();

        expect(worker.messages.at(-1)).toEqual({
            kind: "notifySessionStoreChanged",
        });
        runtime.dispose();
    });

    it("worker fault terminates the worker and runs the full teardown", async () => {
        const worker = new FakeWorker();
        let subscriptionDisposes = 0;
        let chainResponseStops = 0;
        let chainCloses = 0;
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({
                // Manual async iterables whose `return()` records disposal; the
                // provider disposes subscriptions and closes chain connections
                // on a worker fault.
                subscribeTheme: () =>
                    ({
                        [Symbol.asyncIterator]() {
                            return this;
                        },
                        next: () => new Promise(() => {}),
                        return: async () => {
                            subscriptionDisposes += 1;
                            return { done: true, value: undefined };
                        },
                    }) as unknown as AsyncIterable<Result<ThemeVariant, GenericError>>,
                connect: async () => ({
                    send() {},
                    responses: () =>
                        ({
                            [Symbol.asyncIterator]() {
                                return this;
                            },
                            next: () => new Promise(() => {}),
                            return: async () => {
                                chainResponseStops += 1;
                                return { done: true, value: undefined };
                            },
                        }) as unknown as AsyncIterable<string>,
                    close() {
                        chainCloses += 1;
                    },
                }),
            }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "subscriptionStart",
            subId: 1,
            name: "subscribeTheme",
            payload: null,
        });
        worker.emit({
            kind: "chainConnectStart",
            connId: 1,
            genesisHash: "0xab",
        });
        await settle();
        await settle();

        const closes: Error[] = [];
        provider.subscribeClose!((error) => closes.push(error));

        worker.emitError("boom");
        await settle();
        await settle();

        expect(worker.terminated).toBe(true);
        expect(subscriptionDisposes).toBe(1);
        expect(chainResponseStops).toBe(1);
        expect(chainCloses).toBe(1);
        expect(closes.length).toBe(1);
        expect(closes[0].message).toMatch(/boom/);

        // The fault teardown is terminal; a second fault is a no-op.
        worker.emitError("again");
        expect(closes.length).toBe(1);

        let lateClose: Error | null = null;
        provider.subscribeClose!((error) => {
            lateClose = error;
        });
        expect(lateClose).toBeInstanceOf(Error);
        expect(lateClose!.message).toMatch(/boom/);
    });

    it("worker fatalError during init rejects provider creation", async () => {
        const worker = new FakeWorker();
        const providerPromise = createProviderFromRuntime(asWorker(worker), makeHostCallbacks(), {
            runtimeConfig: runtimeConfig(),
        });

        worker.emit({ kind: "fatalError", error: "bad wasm" });

        await expect(providerPromise).rejects.toThrow(/worker init reported error: bad wasm/);
        expect(worker.terminated).toBe(true);
    });

    it("worker frameError after init closes the provider", async () => {
        const worker = new FakeWorker();
        const provider = await readyProvider(worker);
        const closes: Error[] = [];
        provider.subscribeClose!((error) => closes.push(error));

        worker.emit({ kind: "frameError", coreId: 1, error: "bad frame" });

        expect(worker.terminated).toBe(false);
        expect(worker.messages.at(-1)).toEqual({ kind: "disposeCore", coreId: 1 });
        expect(closes.length).toBe(1);
        expect(closes[0].message).toMatch(/worker frame error: bad frame/);

        let lateClose: Error | null = null;
        provider.subscribeClose!((error) => {
            lateClose = error;
        });
        expect(lateClose).toBeInstanceOf(Error);
        provider.dispose();
    });

    it("routes payload-carrying subscriptions by name", async () => {
        const worker = new FakeWorker();
        const keys: Uint8Array[] = [];
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({
                lookupPreimage: async function* (key) {
                    keys.push(key);
                    yield ok(new Uint8Array([1]));
                },
            }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "subscriptionStart",
            subId: 4,
            name: "lookupPreimage",
            payload: new Uint8Array([9, 9]),
        });

        await settle();
        await settle();
        expect(keys).toEqual([new Uint8Array([9, 9])]);
        expect(worker.messages.at(-1)).toEqual({
            kind: "subscriptionItem",
            subId: 4,
            value: new Uint8Array([1]),
        });

        provider.dispose();
    });

    it("never falls through unknown subscription names to another callback", async () => {
        const worker = new FakeWorker();
        let preimageStarts = 0;
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({
                lookupPreimage: (() => {
                    preimageStarts += 1;
                    return () => {};
                }) as unknown as PreimageHost["lookupPreimage"],
            }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "subscriptionStart",
            subId: 5,
            name: "someFutureSubscribe",
            payload: new Uint8Array([1, 2, 3]),
        });

        expect(preimageStarts).toBe(0);
        expect(worker.messages.some((m) => m.kind === "subscriptionItem")).toBe(false);

        provider.dispose();
    });

    it("does not dispatch a payload-carrying subscription without payload", async () => {
        const worker = new FakeWorker();
        let preimageStarts = 0;
        const providerPromise = createProviderFromRuntime(
            asWorker(worker),
            makeHostCallbacks({
                lookupPreimage: (() => {
                    preimageStarts += 1;
                    return () => {};
                }) as unknown as PreimageHost["lookupPreimage"],
            }),
            { runtimeConfig: runtimeConfig() },
        );
        worker.emit({ kind: "loaded" });
        worker.emit({ kind: "ready" });
        const provider = await finishProviderReady(worker, providerPromise);

        worker.emit({
            kind: "subscriptionStart",
            subId: 6,
            name: "lookupPreimage",
            payload: null,
        });

        expect(preimageStarts).toBe(0);

        provider.dispose();
    });

    it("rejects when init times out", async () => {
        const worker = new FakeWorker();
        const providerPromise = createProviderFromRuntime(asWorker(worker), makeHostCallbacks(), {
            runtimeConfig: runtimeConfig(),
            initTimeoutMs: 20,
        });
        worker.emit({ kind: "loaded" });
        await expect(providerPromise).rejects.toThrow(/worker init timed out after 20ms/);
        expect(worker.terminated).toBe(true);
    });

    it("rejects on messageerror during init", async () => {
        const worker = new FakeWorker();
        const providerPromise = createProviderFromRuntime(asWorker(worker), makeHostCallbacks(), {
            runtimeConfig: runtimeConfig(),
        });
        worker.emitMessageError();
        await expect(providerPromise).rejects.toThrow(/could not be deserialized/);
        expect(worker.terminated).toBe(true);
    });

    it("decodes raw v01 push notification payloads", async () => {
        let notification: HostPushNotificationRequest | undefined;
        const callbacks = createWasmRawCallbacks(
            makeHostCallbacks({
                pushNotification: async (request) => {
                    notification = request;
                    return { id: 42 };
                },
            }),
        );

        const encoded = await callbacks.pushNotification!(
            HostPushNotificationRequest.enc({
                text: "Hello!",
                deeplink: undefined,
                scheduledAt: undefined,
            }),
        );

        expect(HostPushNotificationResponse.dec(encoded).id).toBe(42);
        expect(notification).toEqual({
            text: "Hello!",
            deeplink: undefined,
            scheduledAt: undefined,
        });
    });
});
