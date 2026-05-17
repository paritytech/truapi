// Generic dispatcher utilities sit in `@parity/truapi-host`. This module
// re-exports them so hosts that depend on `@parity/truapi-host-shared` get the
// dispatcher entry-point without a separate install.

export {
  createHostServer,
  toFlatResponsePayload,
  toResponsePayload,
} from "@parity/truapi-host";

export type {
  CallContext,
  HostDispatchEntry,
  HostServerHooks,
  RequestEntry,
  SubscriptionCleanup,
  SubscriptionEntry,
  SubscriptionFramePort,
  TrUApiHostServer,
} from "@parity/truapi-host";
