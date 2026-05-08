use parity_scale_codec::{Decode, Encode};

/// 32-byte statement topic.
pub type Topic = [u8; 32];

/// Request to subscribe to statements via a topic filter (RFC 0008).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteStatementStoreSubscribeRequest {
    /// AND: statement must contain every listed topic.
    MatchAll(Vec<Topic>),
    /// OR: statement must contain at least one listed topic.
    MatchAny(Vec<Topic>),
}

/// Page of signed statements delivered by the statement store subscription
/// (RFC 0008). The `is_complete` flag distinguishes the historical-dump phase
/// (`false`) from the live-update phase (`true`).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreSubscribeItem {
    /// Signed statements matching the subscription.
    pub statements: Vec<crate::v01::SignedStatement>,
    /// `false` while the host is still streaming the historical dump (more
    /// pages to follow). `true` once the dump is complete; all subsequent
    /// pages are also `true` and carry only newly-arrived statements.
    pub is_complete: bool,
}

impl TryFrom<crate::v01::RemoteStatementStoreSubscribeRequest>
    for RemoteStatementStoreSubscribeRequest
{
    type Error = ();

    fn try_from(
        value: crate::v01::RemoteStatementStoreSubscribeRequest,
    ) -> Result<Self, Self::Error> {
        Ok(Self::MatchAll(value.topics))
    }
}

impl TryFrom<RemoteStatementStoreSubscribeRequest>
    for crate::v01::RemoteStatementStoreSubscribeRequest
{
    type Error = ();

    fn try_from(value: RemoteStatementStoreSubscribeRequest) -> Result<Self, Self::Error> {
        match value {
            RemoteStatementStoreSubscribeRequest::MatchAll(topics) => Ok(Self { topics }),
            // V0.1 only carries MatchAll semantics; MatchAny cannot round-trip.
            RemoteStatementStoreSubscribeRequest::MatchAny(_) => Err(()),
        }
    }
}

impl TryFrom<crate::v01::RemoteStatementStoreSubscribeItem> for RemoteStatementStoreSubscribeItem {
    type Error = ();

    fn try_from(value: crate::v01::RemoteStatementStoreSubscribeItem) -> Result<Self, Self::Error> {
        // Lifting V0.1 → V0.2: we don't know if the historical dump is complete,
        // so report the conservative `true` (live updates only).
        Ok(Self {
            statements: value.statements,
            is_complete: true,
        })
    }
}

impl TryFrom<RemoteStatementStoreSubscribeItem> for crate::v01::RemoteStatementStoreSubscribeItem {
    type Error = ();

    fn try_from(value: RemoteStatementStoreSubscribeItem) -> Result<Self, Self::Error> {
        // V0.1 has no `is_complete` flag; drop it on the way down.
        Ok(Self {
            statements: value.statements,
        })
    }
}
