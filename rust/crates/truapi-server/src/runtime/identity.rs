//! People-chain identity lookup used to resolve usernames for a paired session.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::chain_runtime::{
    ChainRuntime, wait_for_chain_head_best_hash, wait_for_chain_head_storage_value,
};
use crate::host_logic::identity::{
    PeopleIdentity, decode_people_identity, resources_consumers_storage_key,
};
use crate::host_logic::session::SessionInfo;

use tracing::{debug, instrument, warn};
use truapi::latest::{
    OperationStartedResult, RemoteChainHeadFollowRequest, RemoteChainHeadStorageRequest,
    StorageQueryItem, StorageQueryType,
};

/// Fill in missing usernames by querying the people chain; returns the
/// session unchanged when it already carries a username or no people chain
/// is configured.
#[instrument(skip_all, fields(runtime.method = "session.identity.resolve_with_chain"))]
pub(super) async fn resolve_session_identity_with_chain(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    mut session: SessionInfo,
) -> SessionInfo {
    if session.has_username() || people_chain_genesis_hash == [0; 32] {
        return session;
    }

    let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
    if !lookup_and_apply(
        chain,
        people_chain_genesis_hash,
        preferred_account,
        &mut session,
        "identity",
    )
    .await
        && preferred_account != session.public_key
    {
        let public_key = session.public_key;
        lookup_and_apply(
            chain,
            people_chain_genesis_hash,
            public_key,
            &mut session,
            "root identity",
        )
        .await;
    }

    session
}

/// Look up `account`'s people-chain identity and apply any usernames to
/// `session`; returns whether a username record was found and applied.
async fn lookup_and_apply(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    account: [u8; 32],
    session: &mut SessionInfo,
    label: &str,
) -> bool {
    match lookup_people_identity(chain, people_chain_genesis_hash, account).await {
        Ok(Some(identity)) => {
            debug!(
                account = %hex::encode(account),
                lite_username = identity.lite_username.as_deref().unwrap_or(""),
                full_username = identity.full_username.as_deref().unwrap_or(""),
                "People-chain {label} lookup found username"
            );
            session.apply_usernames(identity.lite_username, identity.full_username);
            true
        }
        Ok(None) => {
            debug!(
                account = %hex::encode(account),
                "People-chain {label} lookup found no consumer record"
            );
            false
        }
        Err(reason) => {
            warn!(
                account = %hex::encode(account),
                %reason,
                "People-chain {label} lookup failed"
            );
            false
        }
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
    let lookup_id = {
        use std::sync::atomic::{AtomicU64, Ordering};

        /// Monotonic salt for local identity lookup follow ids, avoiding
        /// collisions between concurrent People-chain identity lookups.
        static IDENTITY_LOOKUP_COUNTER: AtomicU64 = AtomicU64::new(1);

        IDENTITY_LOOKUP_COUNTER.fetch_add(1, Ordering::Relaxed)
    };
    let follow_id = format!("truapi:identity:{}:{}", lookup_id, hex::encode(account_id),);
    let mut follow = chain.remote_chain_head_follow(
        follow_id.clone(),
        RemoteChainHeadFollowRequest {
            genesis_hash: genesis_hash.clone(),
            with_runtime: false,
        },
    );

    let hash = wait_for_chain_head_best_hash(
        &mut follow,
        "People-chain",
        Duration::from_secs(10),
        Duration::from_secs(2),
    )
    .await?;
    let response = chain
        .remote_chain_head_storage(RemoteChainHeadStorageRequest {
            genesis_hash,
            follow_subscription_id: follow_id,
            hash,
            items: vec![StorageQueryItem {
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
    let Some(value) = wait_for_chain_head_storage_value(
        &mut follow,
        &operation_id,
        &key,
        "People-chain",
        Duration::from_secs(10),
    )
    .await?
    else {
        return Ok(None);
    };
    decode_people_identity(&value).map(Some)
}
