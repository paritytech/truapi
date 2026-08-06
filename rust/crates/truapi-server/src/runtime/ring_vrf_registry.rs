//! Durable, wallet-scoped RFC-0024 ring-VRF registry snapshots.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use parity_scale_codec::{Decode, Encode};
use truapi::v01::{ProductAccountId, RegisteredRingVrfKey, RingLocation};
use truapi_platform::{CoreStorageKey, Platform, normalize_product_identifier};

use crate::host_logic::sso::messages::RingVrfError;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
struct SelectedProvider {
    ring: RingLocation,
    handle: ProductAccountId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
struct RegistrySnapshot {
    entries: Vec<RegisteredRingVrfKey>,
    /// Owners for which `entries` is a complete Account Holder snapshot.
    complete_owners: Vec<String>,
    /// First registration wins until the user selects another provider.
    selected_providers: Vec<SelectedProvider>,
}

/// Shared durable repository used by both account-authority roles.
pub(super) struct RingVrfRegistryStore {
    platform: Arc<dyn Platform>,
    cache: Mutex<HashMap<[u8; 32], RegistrySnapshot>>,
    storage_guard: futures::lock::Mutex<()>,
}

impl RingVrfRegistryStore {
    pub(super) fn new(platform: Arc<dyn Platform>) -> Arc<Self> {
        Arc::new(Self {
            platform,
            cache: Mutex::new(HashMap::new()),
            storage_guard: futures::lock::Mutex::new(()),
        })
    }

    pub(super) async fn entry(
        &self,
        root_public_key: [u8; 32],
        handle: &ProductAccountId,
    ) -> Result<Option<RegisteredRingVrfKey>, RingVrfError> {
        Ok(self
            .snapshot(root_public_key)
            .await?
            .entries
            .into_iter()
            .find(|entry| entry.handle == *handle))
    }

    pub(super) async fn complete_owner_entries(
        &self,
        root_public_key: [u8; 32],
        owner: &str,
    ) -> Result<Option<Vec<RegisteredRingVrfKey>>, RingVrfError> {
        let snapshot = self.snapshot(root_public_key).await?;
        if !snapshot.complete_owners.iter().any(|item| item == owner) {
            return Ok(None);
        }
        Ok(Some(
            snapshot
                .entries
                .into_iter()
                .filter(|entry| entry.handle.dot_ns_identifier == owner)
                .collect(),
        ))
    }

    pub(super) async fn owner_entries(
        &self,
        root_public_key: [u8; 32],
        owner: &str,
    ) -> Result<Vec<RegisteredRingVrfKey>, RingVrfError> {
        Ok(self
            .snapshot(root_public_key)
            .await?
            .entries
            .into_iter()
            .filter(|entry| entry.handle.dot_ns_identifier == owner)
            .collect())
    }

    pub(super) async fn register(
        &self,
        root_public_key: [u8; 32],
        handle: ProductAccountId,
        ring: RingLocation,
        public_key: [u8; 32],
    ) -> Result<(), RingVrfError> {
        let _guard = self.storage_guard.lock().await;
        let mut snapshot = self.load_under_guard(root_public_key).await?;
        if let Some(entry) = snapshot
            .entries
            .iter_mut()
            .find(|entry| entry.handle == handle)
        {
            if entry.public_key != Some(public_key) {
                return Err(invalid_registry(
                    "registered key handle has a conflicting public key",
                ));
            }
            if !entry.rings.contains(&ring) {
                entry.rings.push(ring.clone());
            }
        } else {
            snapshot.entries.push(RegisteredRingVrfKey {
                handle: handle.clone(),
                rings: vec![ring.clone()],
                public_key: Some(public_key),
            });
        }
        if !snapshot
            .selected_providers
            .iter()
            .any(|provider| provider.ring == ring)
        {
            snapshot
                .selected_providers
                .push(SelectedProvider { ring, handle });
        }
        self.persist_under_guard(root_public_key, snapshot).await
    }

    /// Reconcile one owner's complete response with locally registered keys.
    ///
    /// RFC-0024 has no revocation operation, so a remote response cannot
    /// invalidate an entry already accepted by this host. This also prevents a
    /// list response created before a fire-and-forget registration mirror from
    /// removing that local registration.
    pub(super) async fn reconcile_owner(
        &self,
        root_public_key: [u8; 32],
        owner: &str,
        entries: Vec<RegisteredRingVrfKey>,
    ) -> Result<Vec<RegisteredRingVrfKey>, RingVrfError> {
        validate_authoritative_owner_entries(owner, &entries)?;
        let _guard = self.storage_guard.lock().await;
        let mut snapshot = self.load_under_guard(root_public_key).await?;
        let mut owner_entries = snapshot
            .entries
            .iter()
            .filter(|entry| entry.handle.dot_ns_identifier == owner)
            .cloned()
            .collect::<Vec<_>>();
        for entry in entries {
            if let Some(existing) = owner_entries
                .iter_mut()
                .find(|existing| existing.handle == entry.handle)
            {
                if existing.public_key != entry.public_key {
                    return Err(invalid_registry_listing(
                        "owner snapshot conflicts with a locally registered public key",
                    ));
                }
                for ring in entry.rings {
                    if !existing.rings.contains(&ring) {
                        existing.rings.push(ring);
                    }
                }
            } else {
                owner_entries.push(entry);
            }
        }
        snapshot
            .entries
            .retain(|entry| entry.handle.dot_ns_identifier != owner);
        snapshot.entries.extend(owner_entries.iter().cloned());
        if !snapshot.complete_owners.iter().any(|item| item == owner) {
            snapshot.complete_owners.push(owner.to_string());
        }
        snapshot.selected_providers.retain(|provider| {
            snapshot.entries.iter().any(|entry| {
                entry.handle == provider.handle && entry.rings.contains(&provider.ring)
            })
        });
        for entry in &snapshot.entries {
            for ring in &entry.rings {
                if !snapshot
                    .selected_providers
                    .iter()
                    .any(|provider| provider.ring == *ring)
                {
                    snapshot.selected_providers.push(SelectedProvider {
                        ring: ring.clone(),
                        handle: entry.handle.clone(),
                    });
                }
            }
        }
        self.persist_under_guard(root_public_key, snapshot).await?;
        Ok(owner_entries)
    }

    pub(super) async fn selected_provider(
        &self,
        root_public_key: [u8; 32],
        ring: &RingLocation,
    ) -> Result<Option<ProductAccountId>, RingVrfError> {
        Ok(self
            .snapshot(root_public_key)
            .await?
            .selected_providers
            .into_iter()
            .find(|provider| provider.ring == *ring)
            .map(|provider| provider.handle))
    }

    pub(super) async fn providers(
        &self,
        root_public_key: [u8; 32],
        ring: &RingLocation,
    ) -> Result<Vec<ProductAccountId>, RingVrfError> {
        Ok(self
            .snapshot(root_public_key)
            .await?
            .entries
            .into_iter()
            .filter(|entry| entry.rings.contains(ring))
            .map(|entry| entry.handle)
            .collect())
    }

    /// Persist a user-selected provider after validating its registration.
    pub(super) async fn select_provider(
        &self,
        root_public_key: [u8; 32],
        ring: RingLocation,
        handle: ProductAccountId,
    ) -> Result<(), RingVrfError> {
        let _guard = self.storage_guard.lock().await;
        let mut snapshot = self.load_under_guard(root_public_key).await?;
        let registered = snapshot
            .entries
            .iter()
            .any(|entry| entry.handle == handle && entry.rings.contains(&ring));
        if !registered {
            return Err(RingVrfError::KeyNotInRing);
        }
        snapshot
            .selected_providers
            .retain(|provider| provider.ring != ring);
        snapshot
            .selected_providers
            .push(SelectedProvider { ring, handle });
        self.persist_under_guard(root_public_key, snapshot).await
    }

    async fn snapshot(&self, root_public_key: [u8; 32]) -> Result<RegistrySnapshot, RingVrfError> {
        if let Some(snapshot) = self
            .cache
            .lock()
            .expect("ring-VRF registry cache mutex poisoned")
            .get(&root_public_key)
            .cloned()
        {
            return Ok(snapshot);
        }
        let _guard = self.storage_guard.lock().await;
        self.load_under_guard(root_public_key).await
    }

    async fn load_under_guard(
        &self,
        root_public_key: [u8; 32],
    ) -> Result<RegistrySnapshot, RingVrfError> {
        if let Some(snapshot) = self
            .cache
            .lock()
            .expect("ring-VRF registry cache mutex poisoned")
            .get(&root_public_key)
            .cloned()
        {
            return Ok(snapshot);
        }
        let key = CoreStorageKey::RingVrfRegistry { root_public_key };
        let snapshot = match self
            .platform
            .read_core_storage(key.clone())
            .await
            .map_err(storage_error)?
        {
            Some(blob) => match decode_snapshot(&blob) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    let _ = self.platform.clear_core_storage(key).await;
                    return Err(error);
                }
            },
            None => RegistrySnapshot::default(),
        };
        self.cache
            .lock()
            .expect("ring-VRF registry cache mutex poisoned")
            .insert(root_public_key, snapshot.clone());
        Ok(snapshot)
    }

    async fn persist_under_guard(
        &self,
        root_public_key: [u8; 32],
        snapshot: RegistrySnapshot,
    ) -> Result<(), RingVrfError> {
        validate_snapshot(&snapshot)?;
        self.platform
            .write_core_storage(
                CoreStorageKey::RingVrfRegistry { root_public_key },
                snapshot.encode(),
            )
            .await
            .map_err(storage_error)?;
        self.cache
            .lock()
            .expect("ring-VRF registry cache mutex poisoned")
            .insert(root_public_key, snapshot);
        Ok(())
    }
}

fn decode_snapshot(blob: &[u8]) -> Result<RegistrySnapshot, RingVrfError> {
    let mut input = blob;
    let snapshot = RegistrySnapshot::decode(&mut input).map_err(|error| RingVrfError::Unknown {
        reason: format!("invalid persisted ring-VRF registry: {error}"),
    })?;
    if !input.is_empty() {
        return Err(RingVrfError::Unknown {
            reason: "invalid persisted ring-VRF registry: trailing bytes".to_string(),
        });
    }
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot(snapshot: &RegistrySnapshot) -> Result<(), RingVrfError> {
    let mut handles = HashSet::new();
    for entry in &snapshot.entries {
        let owner = normalize_product_identifier(&entry.handle.dot_ns_identifier)
            .map_err(|error| invalid_registry(error.to_string()))?;
        if owner != entry.handle.dot_ns_identifier {
            return Err(invalid_registry("non-canonical key owner"));
        }
        if entry.public_key.is_none() {
            return Err(invalid_registry("stored entry is missing its public key"));
        }
        if entry.rings.is_empty() {
            return Err(invalid_registry("stored entry has no declared rings"));
        }
        if !handles.insert(entry.handle.encode()) {
            return Err(invalid_registry("duplicate key handle"));
        }
        let mut rings = HashSet::new();
        if entry.rings.iter().any(|ring| !rings.insert(ring.encode())) {
            return Err(invalid_registry("duplicate declared ring"));
        }
    }
    let mut owners = HashSet::new();
    for owner in &snapshot.complete_owners {
        let canonical = normalize_product_identifier(owner)
            .map_err(|error| invalid_registry(error.to_string()))?;
        if canonical != *owner || !owners.insert(owner) {
            return Err(invalid_registry("invalid complete owner"));
        }
    }
    let mut provider_rings = HashSet::new();
    for provider in &snapshot.selected_providers {
        if !provider_rings.insert(provider.ring.encode()) {
            return Err(invalid_registry("duplicate selected provider"));
        }
        if !snapshot
            .entries
            .iter()
            .any(|entry| entry.handle == provider.handle && entry.rings.contains(&provider.ring))
        {
            return Err(invalid_registry(
                "selected provider is not registered for its ring",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_owner_listing(
    owner: &str,
    entries: &[RegisteredRingVrfKey],
) -> Result<(), RingVrfError> {
    let canonical_owner = normalize_product_identifier(owner)
        .map_err(|error| invalid_registry_listing(error.to_string()))?;
    if canonical_owner != owner {
        return Err(invalid_registry_listing("non-canonical listing owner"));
    }
    let mut handles = HashSet::new();
    for entry in entries {
        if entry.handle.dot_ns_identifier != owner {
            return Err(invalid_registry_listing(
                "owner listing contains a foreign key handle",
            ));
        }
        if entry.rings.is_empty() {
            return Err(invalid_registry_listing(
                "owner listing contains a key with no rings",
            ));
        }
        if !handles.insert(entry.handle.encode()) {
            return Err(invalid_registry_listing(
                "owner listing contains a duplicate key handle",
            ));
        }
        let mut rings = HashSet::new();
        if entry.rings.iter().any(|ring| !rings.insert(ring.encode())) {
            return Err(invalid_registry_listing(
                "owner listing contains a duplicate declared ring",
            ));
        }
    }
    Ok(())
}

fn validate_authoritative_owner_entries(
    owner: &str,
    entries: &[RegisteredRingVrfKey],
) -> Result<(), RingVrfError> {
    validate_owner_listing(owner, entries)?;
    if entries.iter().any(|entry| entry.public_key.is_none()) {
        return Err(invalid_registry_listing(
            "authoritative owner snapshot contains a key without its public key",
        ));
    }
    Ok(())
}

fn storage_error(error: truapi::v01::GenericError) -> RingVrfError {
    RingVrfError::Unknown {
        reason: format!("ring-VRF registry storage failed: {}", error.reason),
    }
}

fn invalid_registry(reason: impl Into<String>) -> RingVrfError {
    RingVrfError::Unknown {
        reason: format!("invalid persisted ring-VRF registry: {}", reason.into()),
    }
}

fn invalid_registry_listing(reason: impl Into<String>) -> RingVrfError {
    RingVrfError::Unknown {
        reason: format!("invalid ring-VRF registry listing: {}", reason.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::StubPlatform;

    fn handle(owner: &str, index: u32) -> ProductAccountId {
        ProductAccountId {
            dot_ns_identifier: owner.to_string(),
            derivation_index: truapi::v01::DerivationIndex::Left(index),
        }
    }

    fn ring(byte: u8) -> RingLocation {
        RingLocation {
            chain_id: [byte; 32],
            junctions: vec![truapi::v01::RingLocationJunction::PalletInstance(byte)],
        }
    }

    #[test]
    fn registry_is_wallet_scoped_durable_and_registration_is_idempotent() {
        let platform = Arc::new(StubPlatform::default());
        let store = RingVrfRegistryStore::new(platform.clone());
        let first_root = [1; 32];
        let second_root = [2; 32];
        let key = handle("owner.dot", 7);
        futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32]))
            .unwrap();
        futures::executor::block_on(store.register(first_root, key.clone(), ring(1), [9; 32]))
            .unwrap();
        futures::executor::block_on(store.register(first_root, key.clone(), ring(2), [9; 32]))
            .unwrap();

        let entry = futures::executor::block_on(store.entry(first_root, &key))
            .unwrap()
            .unwrap();
        assert_eq!(entry.rings, vec![ring(1), ring(2)]);
        assert!(
            futures::executor::block_on(store.entry(second_root, &key))
                .unwrap()
                .is_none()
        );

        let reloaded = RingVrfRegistryStore::new(platform);
        assert_eq!(
            futures::executor::block_on(reloaded.entry(first_root, &key)).unwrap(),
            Some(entry)
        );
    }

    #[test]
    fn first_registrar_remains_selected_until_explicitly_changed() {
        let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()));
        let root = [1; 32];
        let location = ring(1);
        let first = handle("first.dot", 0);
        let second = handle("second.dot", 0);
        futures::executor::block_on(store.register(root, first.clone(), location.clone(), [1; 32]))
            .unwrap();
        futures::executor::block_on(store.register(
            root,
            second.clone(),
            location.clone(),
            [2; 32],
        ))
        .unwrap();
        assert_eq!(
            futures::executor::block_on(store.selected_provider(root, &location)).unwrap(),
            Some(first.clone())
        );
        assert_eq!(
            futures::executor::block_on(store.providers(root, &location)).unwrap(),
            vec![first.clone(), second.clone()]
        );
        futures::executor::block_on(store.select_provider(root, location.clone(), second.clone()))
            .unwrap();
        assert_eq!(
            futures::executor::block_on(store.selected_provider(root, &location)).unwrap(),
            Some(second)
        );
    }

    #[test]
    fn registration_rejects_a_conflicting_public_key_for_the_same_handle() {
        let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()));
        let root = [1; 32];
        let key = handle("owner.dot", 7);
        futures::executor::block_on(store.register(root, key.clone(), ring(1), [1; 32])).unwrap();

        let error =
            futures::executor::block_on(store.register(root, key.clone(), ring(2), [2; 32]))
                .unwrap_err();

        assert!(matches!(
            error,
            RingVrfError::Unknown { reason }
                if reason.contains("conflicting public key")
        ));
        assert_eq!(
            futures::executor::block_on(store.entry(root, &key))
                .unwrap()
                .unwrap()
                .rings,
            vec![ring(1)]
        );
    }

    #[test]
    fn stale_owner_snapshot_preserves_a_local_registration() {
        let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()));
        let root = [1; 32];
        let local = handle("owner.dot", 7);
        let remote = RegisteredRingVrfKey {
            handle: handle("owner.dot", 8),
            rings: vec![ring(2)],
            public_key: Some([2; 32]),
        };
        futures::executor::block_on(store.register(root, local.clone(), ring(1), [1; 32])).unwrap();

        let reconciled = futures::executor::block_on(store.reconcile_owner(
            root,
            "owner.dot",
            vec![remote.clone()],
        ))
        .unwrap();

        assert_eq!(
            reconciled,
            vec![
                RegisteredRingVrfKey {
                    handle: local,
                    rings: vec![ring(1)],
                    public_key: Some([1; 32]),
                },
                remote,
            ]
        );
        assert_eq!(
            futures::executor::block_on(store.complete_owner_entries(root, "owner.dot")).unwrap(),
            Some(reconciled)
        );
    }

    #[test]
    fn owner_snapshot_rejects_a_conflicting_local_registration() {
        let store = RingVrfRegistryStore::new(Arc::new(StubPlatform::default()));
        let root = [1; 32];
        let key = handle("owner.dot", 7);
        futures::executor::block_on(store.register(root, key.clone(), ring(1), [1; 32])).unwrap();

        let error = futures::executor::block_on(store.reconcile_owner(
            root,
            "owner.dot",
            vec![RegisteredRingVrfKey {
                handle: key.clone(),
                rings: vec![ring(1)],
                public_key: Some([2; 32]),
            }],
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            RingVrfError::Unknown { reason } if reason.contains("conflicts with a locally registered public key")
        ));
        assert_eq!(
            futures::executor::block_on(store.entry(root, &key))
                .unwrap()
                .unwrap()
                .public_key,
            Some([1; 32])
        );
    }

    #[test]
    fn persisted_entries_must_declare_at_least_one_ring() {
        let snapshot = RegistrySnapshot {
            entries: vec![RegisteredRingVrfKey {
                handle: handle("owner.dot", 0),
                rings: vec![],
                public_key: Some([1; 32]),
            }],
            ..RegistrySnapshot::default()
        };

        assert!(matches!(
            decode_snapshot(&snapshot.encode()),
            Err(RingVrfError::Unknown { reason }) if reason.contains("no declared rings")
        ));
    }

    #[test]
    fn owner_listing_rejects_foreign_and_duplicate_entries() {
        let owner = "owner.dot";
        let entry = RegisteredRingVrfKey {
            handle: handle(owner, 0),
            rings: vec![ring(1)],
            public_key: None,
        };
        assert!(validate_owner_listing(owner, std::slice::from_ref(&entry)).is_ok());

        let mut foreign = entry.clone();
        foreign.handle = handle("foreign.dot", 0);
        assert!(matches!(
            validate_owner_listing(owner, &[foreign]),
            Err(RingVrfError::Unknown { reason }) if reason.contains("foreign key handle")
        ));
        assert!(matches!(
            validate_owner_listing(owner, &[entry.clone(), entry]),
            Err(RingVrfError::Unknown { reason }) if reason.contains("duplicate key handle")
        ));
    }
}
