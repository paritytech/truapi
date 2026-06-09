//! Unified [`Contacts`] trait.

use crate::versioned::contacts::{
    HostContactsGetError, HostContactsGetRequest, HostContactsGetResponse,
    HostContactsSubscribeError, HostContactsSubscribeItem, HostContactsSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Read access to the user's host-managed address book (RFC 0014).
///
/// Each contact pairs host-local metadata with a context-scoped map keyed by
/// `ProductAccountId`. By default a product only sees entries for its own
/// context (tier 1); cross-context access requires the
/// `ContactsCrossContext` device permission (tier 2).
pub trait Contacts: Send + Sync {
    /// Retrieve the user's contact list.
    ///
    /// When `context` is `None`, the host filters entries to the calling
    /// product's own context (tier 1). When `context` names a different
    /// product, the host requires `ContactsCrossContext` permission (tier 2).
    ///
    /// ```ts
    /// const result = await truapi.contacts.get({});
    /// assert(result.isOk(), "contacts.get failed:", result);
    /// console.log("contacts:", result.value.contacts);
    /// ```
    #[wire(request_id = 162)]
    async fn get(
        &self,
        _cx: &CallContext,
        _request: HostContactsGetRequest,
    ) -> Result<HostContactsGetResponse, CallError<HostContactsGetError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to contact list updates.
    ///
    /// Delivers the full filtered list on each callback; hosts may debounce.
    /// Uses the same access-tier logic as [`Contacts::get`].
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const contacts = await firstValueFrom(
    ///   from(truapi.contacts.subscribe({ request: {} })),
    /// );
    /// console.log("contacts update:", contacts);
    /// ```
    #[wire(start_id = 164)]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: HostContactsSubscribeRequest,
    ) -> Result<Subscription<HostContactsSubscribeItem>, CallError<HostContactsSubscribeError>>
    {
        Err(CallError::unavailable())
    }
}
