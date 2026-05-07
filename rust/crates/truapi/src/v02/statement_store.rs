use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "sp-compat")]
mod sp_compat;

use crate::v01::Topic;

/// Filter for statement subscriptions, allowing richer topic matching than plain
/// topic vectors. Each position in the filter can be `Some(topic)` to require an
/// exact match or `None` to act as a wildcard.
///
/// Mirrors the `TopicFilter` type from `polkadot-sdk` statement store.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct TopicFilter {
    /// Positional topic matchers. `None` entries act as wildcards.
    pub topics: Vec<Option<Topic>>,
}

impl TryFrom<Vec<crate::v01::Topic>> for TopicFilter {
    type Error = ();

    fn try_from(value: Vec<crate::v01::Topic>) -> Result<Self, Self::Error> {
        Ok(Self {
            topics: value.into_iter().map(Some).collect(),
        })
    }
}

impl TryFrom<TopicFilter> for Vec<crate::v01::Topic> {
    type Error = ();

    fn try_from(value: TopicFilter) -> Result<Self, Self::Error> {
        value
            .topics
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(())
    }
}

pub type RemoteStatementStoreSubscribeRequest = TopicFilter;
pub type RemoteStatementStoreSubscribeItem = Vec<crate::v01::SignedStatement>;
