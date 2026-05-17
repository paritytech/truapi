/* tslint:disable */
/* eslint-disable */

/**
 * JS-callable handle to the TrUAPI core. Constructed once per shell boot.
 */
export class WasmTrUApiCore {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Drop the currently-paired session.
     */
    clearActiveSession(): void;
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
    constructor(callbacks: any);
    /**
     * Push a SCALE-encoded protocol frame into the dispatcher. Responses
     * (and subscription items) flow back through the `emitFrame`
     * callback.
     */
    receiveFromProduct(frame: Uint8Array): Promise<void>;
    /**
     * Push the currently-paired session into the core. Called by the
     * host shell whenever the user pairs / unpairs. `pubkey` must be
     * exactly 32 bytes (sr25519 root public key); usernames may be
     * null / undefined when the identity record carries no value.
     */
    setActiveSession(pubkey: Uint8Array, lite_username?: string | null, full_username?: string | null): void;
}

/**
 * Toggle [`crate::debug_log`] output. Hosts read their `truapi:debug`
 * flag (web: localStorage) and call this once during boot.
 */
export function setDebugEnabled(enabled: boolean): void;
