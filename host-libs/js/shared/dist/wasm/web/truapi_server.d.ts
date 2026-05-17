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

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly ffi_truapi_server_rust_future_cancel_f32: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_f64: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_i16: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_i32: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_i64: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_i8: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_pointer: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_rust_buffer: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_u16: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_u32: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_u64: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_u8: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_cancel_void: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_complete_f32: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_f64: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_i16: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_i32: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_i64: (a: bigint, b: number) => bigint;
    readonly ffi_truapi_server_rust_future_complete_i8: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_pointer: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_rust_buffer: (a: number, b: bigint, c: number) => void;
    readonly ffi_truapi_server_rust_future_complete_u16: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_u32: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_u64: (a: bigint, b: number) => bigint;
    readonly ffi_truapi_server_rust_future_complete_u8: (a: bigint, b: number) => number;
    readonly ffi_truapi_server_rust_future_complete_void: (a: bigint, b: number) => void;
    readonly ffi_truapi_server_rust_future_free_f32: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_f64: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_i16: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_i32: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_i64: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_i8: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_pointer: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_rust_buffer: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_u16: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_u32: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_u64: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_u8: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_free_void: (a: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_f32: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_f64: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_i16: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_i32: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_i64: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_i8: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_pointer: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_rust_buffer: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_u16: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_u32: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_u64: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_u8: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rust_future_poll_void: (a: bigint, b: number, c: bigint) => void;
    readonly ffi_truapi_server_rustbuffer_alloc: (a: number, b: bigint, c: number) => void;
    readonly ffi_truapi_server_rustbuffer_free: (a: number, b: number) => void;
    readonly ffi_truapi_server_rustbuffer_from_bytes: (a: number, b: number, c: number) => void;
    readonly ffi_truapi_server_rustbuffer_reserve: (a: number, b: number, c: bigint, d: number) => void;
    readonly ffi_truapi_server_uniffi_contract_version: () => number;
    readonly __wbg_wasmtruapicore_free: (a: number, b: number) => void;
    readonly setDebugEnabled: (a: number) => void;
    readonly wasmtruapicore_clearActiveSession: (a: number) => void;
    readonly wasmtruapicore_dispose: (a: number) => [number, number];
    readonly wasmtruapicore_new: (a: any) => [number, number, number];
    readonly wasmtruapicore_receiveFromProduct: (a: number, b: number, c: number) => any;
    readonly wasmtruapicore_setActiveSession: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h9f03fbb9b0a11e7f: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h9632106b2100de3d: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__ha3786d1373b8eeae: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__ha3786d1373b8eeae_2: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
