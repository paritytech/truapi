import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

import { createMockHost, mockRuntimeConfig } from "./create-mock-host.js";

// Drives the REAL truapi-server WASM core against createMockHost's callbacks —
// headless, no browser, no worker — to prove the JS↔SCALE↔WASM callback bridge.
// Requires the built WASM artifact (`npm run build:wasm` / `make wasm`); skipped
// when it is absent so a plain `bun test` on a fresh checkout stays green. The
// `host-wasm` CI job builds the WASM and runs this suite.
const wasmUrl = new URL("../../dist/wasm/web/truapi_server_bg.wasm", import.meta.url);
const glueUrl = new URL("../../dist/wasm/web/truapi_server.js", import.meta.url);
const built = existsSync(wasmUrl);

// The `host-wasm` CI job builds the WASM first and sets REQUIRE_WASM=1, so a
// missing artifact (a silent `build:wasm` path/output drift) fails loudly here
// instead of skipping green. A plain local `bun test` leaves REQUIRE_WASM unset
// and skips this suite cleanly on a fresh checkout.
if (process.env.REQUIRE_WASM === "1" && !built) {
    throw new Error(
        `REQUIRE_WASM=1 but the WASM artifact is missing at ${wasmUrl.pathname} — run \`npm run build:wasm\` first.`,
    );
}

const suite = built ? describe : describe.skip;

suite("real WASM core ↔ createMockHost bridge", () => {
    it("the core invokes createMockHost callbacks across the JS↔SCALE↔WASM boundary", async () => {
        const { initSync, WasmHostCore } = await import(glueUrl.href);
        const { createWasmRawCallbacks } = await import("../generated/host-callbacks-adapter.js");
        initSync({ module: readFileSync(wasmUrl) });

        const mock = createMockHost();
        const invoked: string[] = [];
        const readCoreStorage = mock.callbacks.readCoreStorage.bind(mock.callbacks);
        mock.callbacks.readCoreStorage = async (key) => {
            invoked.push(`readCoreStorage:${key.tag}`);
            return readCoreStorage(key);
        };

        const raw = createWasmRawCallbacks(mock.callbacks);
        // The core emits response frames through `emitFrame`; the worker sets it
        // outside the generated adapter, so the harness supplies it too.
        (raw as unknown as { emitFrame: (bytes: Uint8Array) => void }).emitFrame = () => {};
        new WasmHostCore(raw, mockRuntimeConfig());
        // The real core reads its auth session on startup, which crosses the bridge
        // into the mock's readCoreStorage with a SCALE-decoded CoreStorageKey.
        await new Promise((resolve) => setTimeout(resolve, 200));

        expect(invoked.some((c) => c.startsWith("readCoreStorage:"))).toBe(true);
    });
});
