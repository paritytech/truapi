//! Unified [`Contacts`] trait.

use crate::versioned::contacts::{
    HostContactsDeriveSharedKeyError, HostContactsDeriveSharedKeyRequest,
    HostContactsDeriveSharedKeyResponse, HostContactsResolveError, HostContactsResolveRequest,
    HostContactsResolveResponse, HostContactsSendError, HostContactsSendRequest,
    HostContactsSubscribeError, HostContactsSubscribeItem,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Contact requests and invitations between users, mediated by the host
/// (RFC 0022).
pub trait Contacts: Send + Sync {
    /// Resolve a peer to their identity key and published exchange key.
    ///
    /// ```ts
    /// const result = await truapi.contacts.resolve({
    ///   peer: { tag: "Username", value: "alice" },
    /// });
    /// assert(result.isOk(), "resolve failed:", result);
    /// console.log("peer resolved:", result.value);
    /// ```
    #[wire(request_id = 164)]
    async fn resolve(
        &self,
        _cx: &CallContext,
        _request: HostContactsResolveRequest,
    ) -> Result<HostContactsResolveResponse, CallError<HostContactsResolveError>> {
        Err(CallError::unavailable())
    }

    /// Derive a symmetric key shared with a peer, scoped to the calling
    /// product and a caller-chosen context. Both sides derive the same key
    /// from the same context; the exchange secrets never leave the host.
    ///
    /// ```ts
    /// // context: blake2b256("link3:content:v1") — at most 32 bytes.
    /// const result = await truapi.contacts.deriveSharedKey({
    ///   peer: { tag: "Username", value: "alice" },
    ///   context: "0x6c696e6b333a636f6e74656e743a7631",
    /// });
    /// assert(result.isOk(), "deriveSharedKey failed:", result);
    /// console.log("shared key derived:", result.value);
    /// ```
    #[wire(request_id = 166)]
    async fn derive_shared_key(
        &self,
        _cx: &CallContext,
        _request: HostContactsDeriveSharedKeyRequest,
    ) -> Result<HostContactsDeriveSharedKeyResponse, CallError<HostContactsDeriveSharedKeyError>>
    {
        Err(CallError::unavailable())
    }

    /// Seal a payload to a recipient and submit it to their contact inbox.
    /// Triggers the `ContactSend` permission prompt on first use (RFC 0002).
    ///
    /// ```ts
    /// const result = await truapi.contacts.send({
    ///   recipient: { tag: "Username", value: "alice" },
    ///   payload: "0x68656c6c6f",
    /// });
    /// assert(result.isOk(), "send failed:", result);
    /// console.log("contact sent");
    /// ```
    #[wire(request_id = 168)]
    async fn send(
        &self,
        _cx: &CallContext,
        _request: HostContactsSendRequest,
    ) -> Result<(), CallError<HostContactsSendError>> {
        Err(CallError::unavailable())
    }

    /// Receive contact payloads addressed to the calling product. Replays
    /// persisted contacts first (`isComplete: false`), then streams live
    /// ones (`isComplete: true`), as in RFC 0008.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const item = await firstValueFrom(from(truapi.contacts.subscribe()));
    /// console.log("contacts received:", item);
    /// ```
    #[wire(start_id = 170)]
    async fn subscribe(
        &self,
        _cx: &CallContext,
    ) -> Result<Subscription<HostContactsSubscribeItem>, CallError<HostContactsSubscribeError>>
    {
        Err(CallError::unavailable())
    }
}
