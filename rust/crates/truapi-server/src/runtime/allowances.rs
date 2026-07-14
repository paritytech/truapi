//! Persistent allowance-key repository for pairing-host SSO sessions.
//!
//! Implements the host-side allowance cache described in
//! `docs/rfcs/0010-allowance.md`.
//!
//! This mirrors host-papp's allowance repository shape: keys are grouped by
//! SSO session and then indexed by `(product_id, resource)`. The runtime keeps
//! a short-lived memory cache in `PairingHost`; this module owns the durable
//! CoreStorage encoding.

use parity_scale_codec::{Decode, Encode};
use truapi::latest::GenericError;
use truapi_platform::{CoreStorage, CoreStorageKey};

use super::authority::AuthorityError;
use super::sso_remote::SsoSessionKey;
use crate::host_logic::session::{SessionInfo, SsoSessionInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Encode, Decode)]
pub(super) enum AllowanceResource {
    Bulletin,
    StatementStore,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct AllowanceCacheKey {
    session: SsoSessionKey,
    product_id: String,
    resource: AllowanceResource,
}

impl AllowanceCacheKey {
    pub(super) fn new(
        session: &SessionInfo,
        product_id: &str,
        resource: AllowanceResource,
    ) -> Result<Self, AuthorityError> {
        Ok(Self {
            session: sso_cache_key(session)?,
            product_id: product_id.to_string(),
            resource,
        })
    }

    pub(super) fn is_for_session(&self, session: SsoSessionKey) -> bool {
        self.session == session
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct StoredAllowanceEntry {
    product_id: String,
    resource: AllowanceResource,
    slot_account_key: Vec<u8>,
}

pub(super) async fn read_allowance_key(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
    product_id: &str,
    resource: AllowanceResource,
) -> Result<Option<Vec<u8>>, AuthorityError> {
    let entries = read_entries(storage, session).await?;
    Ok(entries
        .into_iter()
        .find(|entry| entry.product_id == product_id && entry.resource == resource)
        .map(|entry| entry.slot_account_key))
}

pub(super) async fn write_allowance_key(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
    product_id: &str,
    resource: AllowanceResource,
    slot_account_key: Vec<u8>,
) -> Result<(), AuthorityError> {
    let mut entries = read_entries(storage, session).await?;
    entries.retain(|entry| !(entry.product_id == product_id && entry.resource == resource));
    entries.push(StoredAllowanceEntry {
        product_id: product_id.to_string(),
        resource,
        slot_account_key,
    });
    storage
        .write_core_storage(storage_key(session)?, encode_entries(entries))
        .await
        .map_err(storage_error)
}

pub(super) async fn remove_allowance_key(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
    product_id: &str,
    resource: AllowanceResource,
) -> Result<(), AuthorityError> {
    let mut entries = read_entries(storage, session).await?;
    let before = entries.len();
    entries.retain(|entry| !(entry.product_id == product_id && entry.resource == resource));
    if entries.len() == before {
        return Ok(());
    }
    storage
        .write_core_storage(storage_key(session)?, encode_entries(entries))
        .await
        .map_err(storage_error)
}

pub(super) async fn clear_session_allowance_keys(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
) -> Result<(), AuthorityError> {
    storage
        .clear_core_storage(storage_key(session)?)
        .await
        .map_err(storage_error)
}

async fn read_entries(
    storage: &(impl CoreStorage + ?Sized),
    session: &SessionInfo,
) -> Result<Vec<StoredAllowanceEntry>, AuthorityError> {
    let Some(blob) = storage
        .read_core_storage(storage_key(session)?)
        .await
        .map_err(storage_error)?
    else {
        return Ok(Vec::new());
    };
    decode_entries(&blob)
}

fn encode_entries(entries: Vec<StoredAllowanceEntry>) -> Vec<u8> {
    entries.encode()
}

fn decode_entries(blob: &[u8]) -> Result<Vec<StoredAllowanceEntry>, AuthorityError> {
    let mut input = blob;
    let entries =
        Vec::<StoredAllowanceEntry>::decode(&mut input).map_err(|err| AuthorityError::Unknown {
            reason: format!("invalid persisted allowance keys: {err}"),
        })?;
    if !input.is_empty() {
        return Err(AuthorityError::Unknown {
            reason: "invalid persisted allowance keys: trailing bytes".to_string(),
        });
    }
    Ok(entries)
}

fn storage_key(session: &SessionInfo) -> Result<CoreStorageKey, AuthorityError> {
    Ok(CoreStorageKey::AllowanceKeys {
        session_id: session_storage_id(session.sso.as_ref().ok_or(AuthorityError::Disconnected)?),
    })
}

fn sso_cache_key(session: &SessionInfo) -> Result<SsoSessionKey, AuthorityError> {
    let sso = session.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
    Ok(SsoSessionKey::from_session(sso))
}

fn session_storage_id(session: &SsoSessionInfo) -> String {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&session.session_id_own);
    bytes.extend_from_slice(&session.session_id_peer);
    hex::encode(bytes)
}

fn storage_error(err: GenericError) -> AuthorityError {
    AuthorityError::Unknown {
        reason: format!("allowance storage failed: {}", err.reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::test_support::sso_session_info;

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    }

    #[truapi_platform::async_trait]
    impl CoreStorage for MemStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, GenericError> {
            Ok(self
                .inner
                .lock()
                .expect("storage mutex poisoned")
                .get(&key.encode())
                .cloned())
        }

        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .insert(key.encode(), value);
            Ok(())
        }

        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError> {
            self.inner
                .lock()
                .expect("storage mutex poisoned")
                .remove(&key.encode());
            Ok(())
        }
    }

    #[test]
    fn stores_allowance_keys_by_product_and_resource() {
        let storage = MemStorage::default();
        let session = sso_session_info();

        futures::executor::block_on(async {
            write_allowance_key(
                &storage,
                &session,
                "dotli.localhost",
                AllowanceResource::Bulletin,
                vec![1; 64],
            )
            .await
            .unwrap();
            write_allowance_key(
                &storage,
                &session,
                "dotli.localhost",
                AllowanceResource::StatementStore,
                vec![2; 64],
            )
            .await
            .unwrap();

            assert_eq!(
                read_allowance_key(
                    &storage,
                    &session,
                    "dotli.localhost",
                    AllowanceResource::Bulletin
                )
                .await
                .unwrap(),
                Some(vec![1; 64])
            );
            assert_eq!(
                read_allowance_key(
                    &storage,
                    &session,
                    "dotli.localhost",
                    AllowanceResource::StatementStore
                )
                .await
                .unwrap(),
                Some(vec![2; 64])
            );
            assert_eq!(
                read_allowance_key(
                    &storage,
                    &session,
                    "other.localhost",
                    AllowanceResource::Bulletin
                )
                .await
                .unwrap(),
                None
            );
        });
    }

    #[test]
    fn clears_session_allowance_keys() {
        let storage = MemStorage::default();
        let session = sso_session_info();

        futures::executor::block_on(async {
            write_allowance_key(
                &storage,
                &session,
                "dotli.localhost",
                AllowanceResource::Bulletin,
                vec![1; 64],
            )
            .await
            .unwrap();
            clear_session_allowance_keys(&storage, &session)
                .await
                .unwrap();
            assert_eq!(
                read_allowance_key(
                    &storage,
                    &session,
                    "dotli.localhost",
                    AllowanceResource::Bulletin
                )
                .await
                .unwrap(),
                None
            );
        });
    }
}
