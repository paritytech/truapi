import type { Payload, ProtocolMessage, Provider } from "@parity/truapi";

export type { Payload, ProtocolMessage, Provider };

/**
 * Subset of permission tags the host can be asked to prompt for. Mirrors
 * the Rust `Permission` enum that flows through the WASM bridge.
 */
export type HostPermissionKind = "Device" | "Remote";
