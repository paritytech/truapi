//! Permission authorization state machine (ask -> authorized | denied), backed
//! by the platform [`CoreStorage`] trait with typed [`CoreStorageKey`] slots.
//!
//! Device permissions (camera, mic, NFC, ...) are separate from remote
//! permissions (domain access, chain submit, ...), so this module exposes two
//! `check_or_prompt` entrypoints that route to the matching platform callback.
//! The cache layer is shared but keys are typed so a device grant cannot
//! authorize a remote operation by accident. Keys are also scoped by product id
//! so one product's authorization never grants another product's request.

use parity_scale_codec::{Decode, Encode};

use truapi::latest::{
    GenericError, HostDevicePermissionRequest, HostDevicePermissionResponse, RemotePermission,
    RemotePermissionRequest, RemotePermissionResponse,
};
use truapi_platform::{
    CoreStorage, CoreStorageKey, PermissionAuthorizationRequest, PermissionAuthorizationStatus,
    Permissions,
};

/// Persisted answer for a single permission request. Keep `Authorized` at
/// discriminant 0 and `Denied` at 1 to preserve the existing two-variant cache
/// encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum StoredAuthorizationStatus {
    /// User authorized the permission.
    Authorized,
    /// User denied the permission.
    Denied,
}

impl From<StoredAuthorizationStatus> for PermissionAuthorizationStatus {
    fn from(status: StoredAuthorizationStatus) -> Self {
        match status {
            StoredAuthorizationStatus::Authorized => PermissionAuthorizationStatus::Authorized,
            StoredAuthorizationStatus::Denied => PermissionAuthorizationStatus::Denied,
        }
    }
}

impl From<bool> for StoredAuthorizationStatus {
    fn from(granted: bool) -> Self {
        if granted {
            Self::Authorized
        } else {
            Self::Denied
        }
    }
}

/// Coordinator that inspects persisted state first, falls back to the
/// platform's prompt callback, and writes the authorization back so future
/// calls short-circuit.
pub struct PermissionsService<'a, S: CoreStorage + ?Sized, P: Permissions + ?Sized> {
    storage: &'a S,
    prompt: &'a P,
    product_id: &'a str,
}

impl<'a, S: CoreStorage + ?Sized, P: Permissions + ?Sized> PermissionsService<'a, S, P> {
    /// Construct a service backed by the given storage + prompt callbacks.
    pub fn new(storage: &'a S, prompt: &'a P, product_id: &'a str) -> Self {
        Self {
            storage,
            prompt,
            product_id,
        }
    }

    /// Returns the stored authorization status for a device permission without prompting.
    pub async fn peek_device(
        &self,
        permission: &HostDevicePermissionRequest,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        authorization_status(
            self.storage,
            device_core_storage_key(self.product_id, permission),
        )
        .await
    }

    /// Returns the stored authorization status for a remote permission without
    /// prompting.
    pub async fn peek_remote(
        &self,
        request: &RemotePermissionRequest,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        authorization_status(
            self.storage,
            remote_core_storage_key(self.product_id, request),
        )
        .await
    }

    /// Returns the stored authorization status for a permission request
    /// without prompting.
    pub async fn authorization_status(
        &self,
        request: &PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        match request {
            PermissionAuthorizationRequest::Device(permission) => {
                self.peek_device(permission).await
            }
            PermissionAuthorizationRequest::Remote(request) => self.peek_remote(request).await,
        }
    }

    /// Returns the stored authorization statuses for permission requests
    /// without prompting. Results follow the same order as `requests`.
    pub async fn authorization_statuses(
        &self,
        requests: &[PermissionAuthorizationRequest],
    ) -> Result<Vec<PermissionAuthorizationStatus>, GenericError> {
        let mut statuses = Vec::with_capacity(requests.len());
        for request in requests {
            statuses.push(self.authorization_status(request).await?);
        }
        Ok(statuses)
    }

    /// Update the stored authorization status for a permission request.
    ///
    /// Setting `NotDetermined` clears the stored value so the next product
    /// request prompts again.
    pub async fn set_authorization_status(
        &self,
        request: &PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), GenericError> {
        let key = match request {
            PermissionAuthorizationRequest::Device(permission) => {
                device_core_storage_key(self.product_id, permission)
            }
            PermissionAuthorizationRequest::Remote(request) => {
                remote_core_storage_key(self.product_id, request)
            }
        };
        set_authorization_status(self.storage, key, status).await
    }

    /// Returns the cached device authorization if any, otherwise prompts the
    /// platform's `device_permission` callback and persists the answer.
    pub async fn check_or_prompt_device(
        &self,
        permission: HostDevicePermissionRequest,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        let key = device_core_storage_key(self.product_id, &permission);
        if let Some(cached) = peek_stored(self.storage, key.clone()).await? {
            return Ok(cached.into());
        }
        // Only a genuine user authorization is persisted. A prompt-callback error is
        // transient (UI unavailable, IPC timeout), not a denial, so fail closed
        // for this call but do not cache it — the next request re-prompts rather
        // than locking the capability out permanently with no revoke path.
        let authorization = match self.prompt.device_permission(permission).await {
            Ok(HostDevicePermissionResponse { granted }) => granted.into(),
            Err(_) => return Ok(PermissionAuthorizationStatus::Denied),
        };
        self.persist_decision(key, authorization).await
    }

    /// Returns the cached remote authorization if any, otherwise prompts the
    /// platform's `remote_permission` callback and persists the answer.
    pub async fn check_or_prompt_remote(
        &self,
        request: RemotePermissionRequest,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        let key = remote_core_storage_key(self.product_id, &request);
        if let Some(cached) = peek_stored(self.storage, key.clone()).await? {
            return Ok(cached.into());
        }
        // See `check_or_prompt_device`: persist only a genuine user decision; a
        // transient callback error fails closed for this call without caching.
        let authorization = match self.prompt.remote_permission(request).await {
            Ok(RemotePermissionResponse { granted }) => granted.into(),
            Err(_) => return Ok(PermissionAuthorizationStatus::Denied),
        };
        self.persist_decision(key, authorization).await
    }

    /// Persist a fresh user decision and return its public status.
    async fn persist_decision(
        &self,
        key: CoreStorageKey,
        authorization: StoredAuthorizationStatus,
    ) -> Result<PermissionAuthorizationStatus, GenericError> {
        self.storage
            .write_core_storage(key, authorization.encode())
            .await?;
        Ok(authorization.into())
    }
}

async fn authorization_status<S: CoreStorage + ?Sized>(
    storage: &S,
    key: CoreStorageKey,
) -> Result<PermissionAuthorizationStatus, GenericError> {
    Ok(peek_stored(storage, key)
        .await?
        .map(Into::into)
        .unwrap_or(PermissionAuthorizationStatus::NotDetermined))
}

async fn peek_stored<S: CoreStorage + ?Sized>(
    storage: &S,
    key: CoreStorageKey,
) -> Result<Option<StoredAuthorizationStatus>, GenericError> {
    let Some(raw) = storage.read_core_storage(key).await? else {
        return Ok(None);
    };
    Ok(StoredAuthorizationStatus::decode(&mut &*raw).ok())
}

async fn set_authorization_status<S: CoreStorage + ?Sized>(
    storage: &S,
    key: CoreStorageKey,
    status: PermissionAuthorizationStatus,
) -> Result<(), GenericError> {
    match status_into_stored(status) {
        Some(stored) => storage.write_core_storage(key, stored.encode()).await,
        None => storage.clear_core_storage(key).await,
    }
}

fn status_into_stored(status: PermissionAuthorizationStatus) -> Option<StoredAuthorizationStatus> {
    match status {
        PermissionAuthorizationStatus::NotDetermined => None,
        PermissionAuthorizationStatus::Denied => Some(StoredAuthorizationStatus::Denied),
        PermissionAuthorizationStatus::Authorized => Some(StoredAuthorizationStatus::Authorized),
    }
}

fn device_core_storage_key(
    product_id: &str,
    permission: &HostDevicePermissionRequest,
) -> CoreStorageKey {
    CoreStorageKey::PermissionAuthorization {
        product_id: product_id.to_string(),
        request: PermissionAuthorizationRequest::Device(*permission),
    }
}

fn remote_core_storage_key(product_id: &str, request: &RemotePermissionRequest) -> CoreStorageKey {
    CoreStorageKey::PermissionAuthorization {
        product_id: product_id.to_string(),
        request: PermissionAuthorizationRequest::Remote(canonical_remote_request(request)),
    }
}

fn canonical_remote_request(request: &RemotePermissionRequest) -> RemotePermissionRequest {
    let permission = match &request.permission {
        RemotePermission::Remote { domains } => {
            // DNS domains are case-insensitive, so a logically-identical bundle
            // requested with different casing or duplicate entries must
            // canonicalize to one key (no spurious re-prompt).
            let mut canonical: Vec<String> =
                domains.iter().map(|d| d.to_ascii_lowercase()).collect();
            canonical.sort();
            canonical.dedup();
            RemotePermission::Remote { domains: canonical }
        }
        other => other.clone(),
    };
    RemotePermissionRequest { permission }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::lock::Mutex;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use truapi::v01;
    use truapi::v01::GenericError;

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<String, Vec<u8>>>,
    }

    #[truapi_platform::async_trait]
    impl CoreStorage for MemStorage {
        async fn read_core_storage(
            &self,
            key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, v01::GenericError> {
            Ok(self.inner.lock().await.get(&test_key(key)).cloned())
        }
        async fn write_core_storage(
            &self,
            key: CoreStorageKey,
            value: Vec<u8>,
        ) -> Result<(), v01::GenericError> {
            self.inner.lock().await.insert(test_key(key), value);
            Ok(())
        }
        async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
            self.inner.lock().await.remove(&test_key(key));
            Ok(())
        }
    }

    fn test_key(key: CoreStorageKey) -> String {
        hex::encode(key.encode())
    }

    struct ScriptedPrompt {
        device_answers: Mutex<Vec<bool>>,
        remote_answers: Mutex<Vec<bool>>,
        device_calls: AtomicUsize,
        remote_calls: AtomicUsize,
    }

    impl ScriptedPrompt {
        fn new(device_answers: Vec<bool>, remote_answers: Vec<bool>) -> Self {
            Self {
                device_answers: Mutex::new(device_answers),
                remote_answers: Mutex::new(remote_answers),
                device_calls: AtomicUsize::new(0),
                remote_calls: AtomicUsize::new(0),
            }
        }
    }

    #[truapi_platform::async_trait]
    impl Permissions for ScriptedPrompt {
        async fn device_permission(
            &self,
            _request: HostDevicePermissionRequest,
        ) -> Result<HostDevicePermissionResponse, GenericError> {
            self.device_calls.fetch_add(1, Ordering::SeqCst);
            let granted = self
                .device_answers
                .lock()
                .await
                .pop()
                .expect("ScriptedPrompt ran out of device answers");
            Ok(v01::HostDevicePermissionResponse { granted })
        }

        async fn remote_permission(
            &self,
            _request: RemotePermissionRequest,
        ) -> Result<RemotePermissionResponse, GenericError> {
            self.remote_calls.fetch_add(1, Ordering::SeqCst);
            let granted = self
                .remote_answers
                .lock()
                .await
                .pop()
                .expect("ScriptedPrompt ran out of remote answers");
            Ok(v01::RemotePermissionResponse { granted })
        }
    }

    #[test]
    fn core_storage_key_separates_product_device_and_remote_variants() {
        let camera = device_core_storage_key("product.dot", &HostDevicePermissionRequest::Camera);
        let other_product =
            device_core_storage_key("other.dot", &HostDevicePermissionRequest::Camera);
        let remote = remote_core_storage_key(
            "product.dot",
            &RemotePermissionRequest {
                permission: RemotePermission::ChainSubmit,
            },
        );

        assert_ne!(camera, other_product);
        assert_ne!(camera, remote);
    }

    #[test]
    fn remote_core_storage_key_canonicalizes_domain_sets() {
        let unsorted = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["b.example.com".into(), "a.example.com".into()],
            },
        };
        let sorted = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["a.example.com".into(), "b.example.com".into()],
            },
        };
        assert_eq!(
            remote_core_storage_key("product.dot", &unsorted),
            remote_core_storage_key("product.dot", &sorted)
        );

        let mixed = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["Example.COM".into(), "a.com".into(), "a.com".into()],
            },
        };
        let canonical = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["a.com".into(), "example.com".into()],
            },
        };
        assert_eq!(
            remote_core_storage_key("product.dot", &mixed),
            remote_core_storage_key("product.dot", &canonical)
        );
    }

    #[test]
    fn remote_core_storage_key_handles_separator_chars_in_domains() {
        // Domain strings containing separator-looking text must not be able to
        // forge a key that matches an unrelated permission.
        let injecting = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["a|b".into(), "c,d".into(), "remote:web-rtc".into()],
            },
        };
        let benign_same_set = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["x".into(), "y".into(), "z".into()],
            },
        };
        let injecting_key = remote_core_storage_key("product.dot", &injecting);
        let benign_key = remote_core_storage_key("product.dot", &benign_same_set);
        assert_ne!(injecting_key, benign_key);

        // The injecting permission must also be distinct from the `WebRtc`
        // variant it tries to impersonate via crafted strings.
        let webrtc = RemotePermissionRequest {
            permission: RemotePermission::WebRtc,
        };
        assert_ne!(
            injecting_key,
            remote_core_storage_key("product.dot", &webrtc)
        );

        // Re-ordering the same domains still collapses to a single key
        // (canonicalization is order-independent).
        let injecting_reordered = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["remote:web-rtc".into(), "c,d".into(), "a|b".into()],
            },
        };
        assert_eq!(
            injecting_key,
            remote_core_storage_key("product.dot", &injecting_reordered)
        );
    }

    #[test]
    fn check_or_prompt_device_caches_grant() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let first = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let second = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        assert_eq!(first, PermissionAuthorizationStatus::Authorized);
        assert_eq!(second, PermissionAuthorizationStatus::Authorized);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn check_or_prompt_remote_caches_denial() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![false]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let request = RemotePermissionRequest {
            permission: RemotePermission::ChainSubmit,
        };
        let first =
            futures::executor::block_on(service.check_or_prompt_remote(request.clone())).unwrap();
        let second = futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap();

        assert_eq!(first, PermissionAuthorizationStatus::Denied);
        assert_eq!(second, PermissionAuthorizationStatus::Denied);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn device_and_remote_caches_are_independent() {
        let storage = MemStorage::default();
        // Device denies, remote grants. If the caches collided we'd see the
        // same answer on the second call.
        let prompt = ScriptedPrompt::new(vec![false], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let device = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let remote =
            futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
                permission: RemotePermission::ChainSubmit,
            }))
            .unwrap();

        assert_eq!(device, PermissionAuthorizationStatus::Denied);
        assert_eq!(remote, PermissionAuthorizationStatus::Authorized);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn device_prompt_does_not_invoke_remote_callback() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let _ = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn remote_prompt_does_not_invoke_device_callback() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![true]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let _ =
            futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
                permission: RemotePermission::WebRtc,
            }))
            .unwrap();
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn peek_returns_not_determined_until_authorized() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let before =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(before, PermissionAuthorizationStatus::NotDetermined);

        futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        let after =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(after, PermissionAuthorizationStatus::Authorized);
    }

    #[test]
    fn set_authorization_status_writes_and_clears() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");
        let request = PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera);

        futures::executor::block_on(
            service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
        )
        .unwrap();
        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::Authorized
        );

        futures::executor::block_on(
            service
                .set_authorization_status(&request, PermissionAuthorizationStatus::NotDetermined),
        )
        .unwrap();
        assert_eq!(
            futures::executor::block_on(service.authorization_status(&request)).unwrap(),
            PermissionAuthorizationStatus::NotDetermined
        );
    }

    /// Prompt callback that always errors, to exercise the transient-failure
    /// path (fail closed for the current call, but do not persist the error).
    struct FailingPrompt;

    #[truapi_platform::async_trait]
    impl Permissions for FailingPrompt {
        async fn device_permission(
            &self,
            _request: HostDevicePermissionRequest,
        ) -> Result<HostDevicePermissionResponse, GenericError> {
            Err(GenericError {
                reason: "boom".into(),
            })
        }

        async fn remote_permission(
            &self,
            _request: RemotePermissionRequest,
        ) -> Result<RemotePermissionResponse, GenericError> {
            Err(GenericError {
                reason: "boom".into(),
            })
        }
    }

    #[test]
    fn prompt_failure_denies_without_persisting() {
        let storage = MemStorage::default();
        let prompt = FailingPrompt;
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let decision = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        assert_eq!(decision, PermissionAuthorizationStatus::Denied);

        // A transient callback error is fail-closed for this call but NOT
        // cached, so peek still sees no authorization and the next request
        // re-prompts rather than permanently locking out the capability.
        let cached =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(
            cached,
            PermissionAuthorizationStatus::NotDetermined,
            "a transient prompt error must not be persisted"
        );
    }

    /// A corrupt SCALE-encoded cache entry must be treated as "no cache",
    /// not panic. The service falls back to prompting.
    #[test]
    fn corrupt_cache_entry_returns_none() {
        let storage = MemStorage::default();
        // Write garbage bytes under the canonical key.
        futures::executor::block_on(storage.write_core_storage(
            device_core_storage_key("product.dot", &HostDevicePermissionRequest::Camera),
            vec![0xff, 0xfe, 0xfd],
        ))
        .unwrap();

        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let peeked =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(
            peeked,
            PermissionAuthorizationStatus::NotDetermined,
            "corrupt entry must decode as absent"
        );
    }

    /// Storage failures must propagate to the caller; the service must not
    /// swallow them by silently returning a default authorization.
    #[derive(Default)]
    struct FailingStorage;

    #[truapi_platform::async_trait]
    impl CoreStorage for FailingStorage {
        async fn read_core_storage(
            &self,
            _key: CoreStorageKey,
        ) -> Result<Option<Vec<u8>>, v01::GenericError> {
            Err(v01::GenericError {
                reason: "read failed".into(),
            })
        }
        async fn write_core_storage(
            &self,
            _key: CoreStorageKey,
            _value: Vec<u8>,
        ) -> Result<(), v01::GenericError> {
            Err(v01::GenericError {
                reason: "write failed".into(),
            })
        }
        async fn clear_core_storage(&self, _key: CoreStorageKey) -> Result<(), v01::GenericError> {
            Err(v01::GenericError {
                reason: "clear failed".into(),
            })
        }
    }

    #[test]
    fn storage_read_error_propagates() {
        let storage = FailingStorage;
        let prompt = ScriptedPrompt::new(vec![], vec![]);
        let service = PermissionsService::new(&storage, &prompt, "product.dot");

        let err = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .expect_err("read failure must surface");
        assert!(matches!(err, v01::GenericError { .. }));
    }
}
