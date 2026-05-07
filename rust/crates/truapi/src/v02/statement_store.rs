use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "sp-compat")]
mod sp_compat;

/// Request to subscribe to statements, allowing richer topic matching than
/// plain topic vectors. Each position in the filter can be `Some(topic)` to
/// require an exact match or `None` to act as a wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreSubscribeRequest {
    /// Positional topic matchers. `None` entries act as wildcards.
    pub topics: Vec<Option<[u8; 32]>>,
}

/// Item containing statements delivered by the statement store subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreSubscribeItem {
    /// Signed statements matching the subscription.
    pub statements: Vec<crate::v01::SignedStatement>,
}

impl TryFrom<Vec<[u8; 32]>> for RemoteStatementStoreSubscribeRequest {
    type Error = ();

    fn try_from(value: Vec<[u8; 32]>) -> Result<Self, Self::Error> {
        Ok(Self {
            topics: value.into_iter().map(Some).collect(),
        })
    }
}

impl TryFrom<RemoteStatementStoreSubscribeRequest> for Vec<[u8; 32]> {
    type Error = ();

    fn try_from(value: RemoteStatementStoreSubscribeRequest) -> Result<Self, Self::Error> {
        value
            .topics
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(())
    }
}

impl TryFrom<crate::v01::RemoteStatementStoreSubscribeRequest>
    for RemoteStatementStoreSubscribeRequest
{
    type Error = ();

    fn try_from(
        value: crate::v01::RemoteStatementStoreSubscribeRequest,
    ) -> Result<Self, Self::Error> {
        Self::try_from(value.topics)
    }
}

impl TryFrom<RemoteStatementStoreSubscribeRequest>
    for crate::v01::RemoteStatementStoreSubscribeRequest
{
    type Error = ();

    fn try_from(value: RemoteStatementStoreSubscribeRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            topics: Vec::<[u8; 32]>::try_from(value)?,
        })
    }
}

impl TryFrom<crate::v01::RemoteStatementStoreSubscribeItem> for RemoteStatementStoreSubscribeItem {
    type Error = ();

    fn try_from(value: crate::v01::RemoteStatementStoreSubscribeItem) -> Result<Self, Self::Error> {
        Ok(Self {
            statements: value.statements,
        })
    }
}

impl TryFrom<RemoteStatementStoreSubscribeItem> for crate::v01::RemoteStatementStoreSubscribeItem {
    type Error = ();

    fn try_from(value: RemoteStatementStoreSubscribeItem) -> Result<Self, Self::Error> {
        Ok(Self {
            statements: value.statements,
        })
    }
}
