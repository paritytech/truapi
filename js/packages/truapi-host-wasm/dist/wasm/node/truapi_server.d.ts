/* tslint:disable */
/* eslint-disable */

/**
 * JS-callable handle to the TrUAPI core. Constructed once per shell boot.
 */
export class WasmTrUApiCore {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Core-owned logout/disconnect. Best-effort notifies the SSO peer when
     * the session has channel material, then clears in-memory and persisted
     * session state.
     */
    disconnect(): Promise<void>;
    /**
     * Tear down the bridge. Invokes the JS-side `dispose` callback so the
     * host can drop its end of the wiring.
     */
    dispose(): void;
    /**
     * Build the core from a JS callbacks object. The object must define
     * every host capability the [`truapi_platform::Platform`] trait set
     * requires (camelCase property names; see the source for the full
     * list).
     */
    constructor(callbacks: any, runtime_config: any);
    /**
     * Push a SCALE-encoded protocol frame into the dispatcher. Responses
     * (and subscription items) flow back through the `emitFrame`
     * callback.
     */
    receiveFromProduct(frame: Uint8Array): Promise<void>;
}

/**
 * Toggle [`crate::debug_log`] output. Hosts read their `truapi:debug`
 * flag (web: localStorage) and call this once during boot.
 */
export function setDebugEnabled(enabled: boolean): void;
