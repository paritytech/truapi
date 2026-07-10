//! People-chain identity lookup used to resolve usernames for a paired session.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::chain_runtime::ChainRuntime;
use crate::host_logic::identity::{
    PeopleIdentity, decode_people_identity, resources_consumers_storage_key,
};
use crate::host_logic::session::SessionInfo;

use futures::{FutureExt, pin_mut};
use subxt::backend::Backend;
use tracing::{debug, instrument, warn};

/// Budget for the whole People-chain lookup (finalized block + storage read).
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

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
    let lookup = fetch_consumer_record(chain, people_chain_genesis_hash, account_id).fuse();
    let timeout = futures_timer::Delay::new(LOOKUP_TIMEOUT).fuse();
    pin_mut!(lookup, timeout);
    let value = futures::select! {
        value = lookup => value?,
        () = timeout => return Err("People-chain identity lookup timed out".to_string()),
    };
    match value {
        Some(value) => decode_people_identity(&value).map(Some),
        None => Ok(None),
    }
}

/// Read the raw `Resources.Consumers` record for `account_id` at the latest
/// finalized block. The key is built locally, so the read never needs the
/// People-chain metadata.
async fn fetch_consumer_record(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    account_id: [u8; 32],
) -> Result<Option<Vec<u8>>, String> {
    let key = resources_consumers_storage_key(&account_id);
    let backend = chain
        .chain_head_backend(&people_chain_genesis_hash)
        .await
        .map_err(|failure| failure.reason())?;
    let at = backend
        .latest_finalized_block_ref()
        .await
        .map_err(|error| format!("People-chain finalized block unavailable: {error}"))?;
    let mut values = backend
        .storage_fetch_values(vec![key.clone()], at.hash())
        .await
        .map_err(|error| format!("People-chain storage lookup failed: {error}"))?;
    let mut value = None;
    while let Some(item) = values.next().await {
        let item = item.map_err(|error| format!("People-chain storage lookup failed: {error}"))?;
        if item.key == key {
            value = Some(item.value);
        }
    }
    Ok(value)
}
