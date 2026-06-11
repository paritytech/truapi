//! People-chain identity lookup used to resolve usernames for a paired
//! session.

use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::PlatformRuntimeHost;
use crate::chain_runtime::ChainRuntime;
use crate::host_logic::identity::{
    PeopleIdentity, decode_people_identity, resources_consumers_storage_key,
};
use crate::host_logic::session::SessionInfo;

use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use tracing::{debug, instrument, warn};
use truapi::v01;
use truapi::v01::{
    OperationStartedResult, RemoteChainHeadFollowItem as V01RemoteChainHeadFollowItem,
    StorageQueryType,
};
use truapi_platform::Platform;

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    /// Resolve usernames for `session` against this runtime's people chain.
    #[instrument(skip_all, fields(runtime.method = "session.identity.resolve"))]
    pub(super) async fn resolve_session_identity(&self, session: SessionInfo) -> SessionInfo {
        resolve_session_identity_with_chain(
            &self.chain,
            self.runtime_config.people_chain_genesis_hash,
            session,
        )
        .await
    }
}

static IDENTITY_LOOKUP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Fill in missing usernames by querying the people chain; returns the
/// session unchanged when it already carries a username or no people chain
/// is configured.
#[instrument(skip_all, fields(runtime.method = "session.identity.resolve_with_chain"))]
pub(super) async fn resolve_session_identity_with_chain(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    mut session: SessionInfo,
) -> SessionInfo {
    if session_has_username(&session) || people_chain_genesis_hash == [0; 32] {
        return session;
    }

    let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
    match lookup_people_identity(chain, people_chain_genesis_hash, preferred_account).await {
        Ok(Some(identity)) => {
            debug!(
                account = %hex::encode(preferred_account),
                lite_username = identity.lite_username.as_deref().unwrap_or(""),
                full_username = identity.full_username.as_deref().unwrap_or(""),
                "People-chain identity lookup found username"
            );
            apply_people_identity(&mut session, identity);
            return session;
        }
        Ok(None) => debug!(
            account = %hex::encode(preferred_account),
            "People-chain identity lookup found no consumer record"
        ),
        Err(reason) => warn!(
            account = %hex::encode(preferred_account),
            %reason,
            "People-chain identity lookup failed"
        ),
    }

    if preferred_account != session.public_key {
        match lookup_people_identity(chain, people_chain_genesis_hash, session.public_key).await {
            Ok(Some(identity)) => {
                debug!(
                    account = %hex::encode(session.public_key),
                    lite_username = identity.lite_username.as_deref().unwrap_or(""),
                    full_username = identity.full_username.as_deref().unwrap_or(""),
                    "People-chain root identity lookup found username"
                );
                apply_people_identity(&mut session, identity);
            }
            Ok(None) => debug!(
                account = %hex::encode(session.public_key),
                "People-chain root identity lookup found no consumer record"
            ),
            Err(reason) => warn!(
                account = %hex::encode(session.public_key),
                %reason,
                "People-chain root identity lookup failed"
            ),
        }
    }

    session
}

fn session_has_username(session: &SessionInfo) -> bool {
    session
        .full_username
        .as_ref()
        .is_some_and(|value| !value.is_empty())
        || session
            .lite_username
            .as_ref()
            .is_some_and(|value| !value.is_empty())
}

fn apply_people_identity(session: &mut SessionInfo, identity: PeopleIdentity) {
    if identity
        .full_username
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        session.full_username = identity.full_username;
    }
    if identity
        .lite_username
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        session.lite_username = identity.lite_username;
    }
}

#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_people_identity(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    account_id: [u8; 32],
) -> Result<Option<PeopleIdentity>, String> {
    let genesis_hash = people_chain_genesis_hash.to_vec();
    let key = resources_consumers_storage_key(&account_id);
    let follow_id = format!(
        "truapi:identity:{}:{}",
        IDENTITY_LOOKUP_COUNTER.fetch_add(1, Ordering::Relaxed),
        hex::encode(account_id),
    );
    let mut follow = chain.remote_chain_head_follow(
        follow_id.clone(),
        v01::RemoteChainHeadFollowRequest {
            genesis_hash: genesis_hash.clone(),
            with_runtime: false,
        },
    );

    let hash = wait_for_identity_follow_hash(&mut follow).await?;
    let response = chain
        .remote_chain_head_storage(v01::RemoteChainHeadStorageRequest {
            genesis_hash,
            follow_subscription_id: follow_id,
            hash,
            items: vec![v01::StorageQueryItem {
                key: key.clone(),
                query_type: StorageQueryType::Value,
            }],
            child_trie: None,
        })
        .await
        .map_err(|failure| failure.reason())?;

    let operation_id = match response.operation {
        OperationStartedResult::Started { operation_id } => operation_id,
        OperationStartedResult::LimitReached => {
            return Err("People-chain storage lookup limit reached".to_string());
        }
    };
    let Some(value) = wait_for_identity_storage_value(&mut follow, &operation_id, &key).await?
    else {
        return Ok(None);
    };
    decode_people_identity(&value).map(Some)
}

#[instrument(skip_all, fields(runtime.method = "session.identity.wait_follow_hash"))]
async fn wait_for_identity_follow_hash(
    follow: &mut BoxStream<'static, V01RemoteChainHeadFollowItem>,
) -> Result<Vec<u8>, String> {
    let timeout = futures_timer::Delay::new(Duration::from_secs(10)).fuse();
    pin_mut!(timeout);
    loop {
        let next = follow.next().fuse();
        pin_mut!(next);
        futures::select! {
            item = next => match item {
                Some(V01RemoteChainHeadFollowItem::Initialized { finalized_block_hashes, .. }) => {
                    let fallback = finalized_block_hashes.last().cloned();
                    return wait_for_identity_best_hash(follow, fallback).await;
                }
                Some(V01RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash }) => {
                    return Ok(best_block_hash);
                }
                Some(V01RemoteChainHeadFollowItem::Stop) | None => {
                    return Err("People-chain follow stopped before initialization".to_string());
                }
                _ => {}
            },
            () = timeout => return Err("People-chain follow initialization timed out".to_string()),
        }
    }
}

async fn wait_for_identity_best_hash(
    follow: &mut BoxStream<'static, V01RemoteChainHeadFollowItem>,
    fallback: Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let timeout = futures_timer::Delay::new(Duration::from_secs(2)).fuse();
    pin_mut!(timeout);
    let mut candidate = fallback;
    loop {
        let next = follow.next().fuse();
        pin_mut!(next);
        futures::select! {
            item = next => match item {
                Some(V01RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash }) => {
                    return Ok(best_block_hash);
                }
                Some(V01RemoteChainHeadFollowItem::NewBlock { block_hash, .. }) => {
                    candidate = Some(block_hash);
                }
                Some(V01RemoteChainHeadFollowItem::Stop) | None => {
                    return candidate.ok_or_else(|| {
                        "People-chain follow stopped before best block".to_string()
                    });
                }
                _ => {}
            },
            () = timeout => {
                return candidate.ok_or_else(|| {
                    "People-chain follow best block timed out".to_string()
                });
            },
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "session.identity.wait_storage_value"))]
async fn wait_for_identity_storage_value(
    follow: &mut BoxStream<'static, V01RemoteChainHeadFollowItem>,
    operation_id: &str,
    key: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let timeout = futures_timer::Delay::new(Duration::from_secs(10)).fuse();
    pin_mut!(timeout);
    let mut value = None;
    loop {
        let next = follow.next().fuse();
        pin_mut!(next);
        futures::select! {
            item = next => match item {
                Some(V01RemoteChainHeadFollowItem::OperationStorageItems { operation_id: item_operation_id, items })
                    if item_operation_id == operation_id =>
                {
                    for item in items {
                        if item.key == key {
                            value = item.value;
                        }
                    }
                }
                Some(V01RemoteChainHeadFollowItem::OperationStorageDone { operation_id: item_operation_id })
                    if item_operation_id == operation_id =>
                {
                    return Ok(value);
                }
                Some(V01RemoteChainHeadFollowItem::OperationInaccessible { operation_id: item_operation_id })
                    if item_operation_id == operation_id =>
                {
                    return Ok(None);
                }
                Some(V01RemoteChainHeadFollowItem::OperationError { operation_id: item_operation_id, error })
                    if item_operation_id == operation_id =>
                {
                    return Err(error);
                }
                Some(V01RemoteChainHeadFollowItem::Stop) | None => {
                    return Err("People-chain follow stopped during storage lookup".to_string());
                }
                _ => {}
            },
            () = timeout => return Err("People-chain storage lookup timed out".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[test]
    fn identity_follow_prefers_best_block_after_initialization() {
        let mut follow = stream::iter(vec![
            V01RemoteChainHeadFollowItem::Initialized {
                finalized_block_hashes: vec![vec![0x01]],
                finalized_block_runtime: None,
            },
            V01RemoteChainHeadFollowItem::BestBlockChanged {
                best_block_hash: vec![0x02],
            },
        ])
        .boxed();

        let hash = futures::executor::block_on(wait_for_identity_follow_hash(&mut follow))
            .expect("best hash should resolve");

        assert_eq!(hash, vec![0x02]);
    }

    #[test]
    fn identity_follow_uses_new_block_before_stale_finalized_fallback() {
        let mut follow = stream::iter(vec![
            V01RemoteChainHeadFollowItem::Initialized {
                finalized_block_hashes: vec![vec![0x01]],
                finalized_block_runtime: None,
            },
            V01RemoteChainHeadFollowItem::NewBlock {
                block_hash: vec![0x03],
                parent_block_hash: vec![0x01],
                new_runtime: None,
            },
            V01RemoteChainHeadFollowItem::Stop,
        ])
        .boxed();

        let hash = futures::executor::block_on(wait_for_identity_follow_hash(&mut follow))
            .expect("new block hash should resolve");

        assert_eq!(hash, vec![0x03]);
    }
}
