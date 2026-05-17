//! Permission state machine (ask -> granted | denied), backed by the platform
//! [`Storage`] trait with a reserved `truapi:permissions:` key prefix.
//!
//! The v0.1 wire protocol keeps device permissions (camera, mic, NFC, ...)
//! separate from remote permissions (domain access, chain submit, ...), so
//! this module exposes two `check_or_prompt` entrypoints that route to the
//! matching platform callback. The cache layer is shared but keys live in
//! distinct sub-namespaces so a device grant cannot authorize a remote
//! operation by accident.

use parity_scale_codec::{Decode, Encode};

use truapi::v01::{
    HostDevicePermissionRequest, HostDevicePermissionResponse, RemotePermission,
    RemotePermissionRequest, RemotePermissionResponse,
};
use truapi_platform::{Permissions, Storage, StorageError};

/// Reserved key prefix for permission state. Hosts must not use keys under
/// this prefix for anything else so core can own the namespace.
pub const PERMISSION_KEY_PREFIX: &str = "truapi:permissions:";

/// Persisted answer for a single permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Decision {
    /// User granted the permission.
    Granted,
    /// User denied the permission.
    Denied,
}

/// Coordinator that inspects persisted state first, falls back to the
/// platform's prompt callback, and writes the decision back so future calls
/// short-circuit.
pub struct PermissionsService<'a> {
    storage: &'a dyn Storage,
    prompt: &'a dyn Permissions,
}

impl<'a> PermissionsService<'a> {
    /// Construct a service backed by the given storage + prompt callbacks.
    pub fn new(storage: &'a dyn Storage, prompt: &'a dyn Permissions) -> Self {
        Self { storage, prompt }
    }

    /// Returns the stored decision for a device permission without prompting.
    pub async fn peek_device(
        &self,
        permission: &HostDevicePermissionRequest,
    ) -> Result<Option<Decision>, StorageError> {
        peek(self.storage, &device_storage_key(permission)).await
    }

    /// Returns the stored decision for a remote permission bundle without
    /// prompting.
    pub async fn peek_remote(
        &self,
        request: &RemotePermissionRequest,
    ) -> Result<Option<Decision>, StorageError> {
        peek(self.storage, &remote_storage_key(request)).await
    }

    /// Returns the cached device decision if any, otherwise prompts the
    /// platform's `device_permission` callback and persists the answer.
    pub async fn check_or_prompt_device(
        &self,
        permission: HostDevicePermissionRequest,
    ) -> Result<Decision, StorageError> {
        let key = device_storage_key(&permission);
        if let Some(cached) = peek(self.storage, &key).await? {
            return Ok(cached);
        }
        let granted = match self.prompt.device_permission(permission).await {
            Ok(HostDevicePermissionResponse { granted }) => granted,
            Err(_) => false,
        };
        let decision = if granted {
            Decision::Granted
        } else {
            Decision::Denied
        };
        self.storage.write(key, decision.encode()).await?;
        Ok(decision)
    }

    /// Returns the cached remote decision if any, otherwise prompts the
    /// platform's `remote_permission` callback and persists the answer.
    pub async fn check_or_prompt_remote(
        &self,
        request: RemotePermissionRequest,
    ) -> Result<Decision, StorageError> {
        let key = remote_storage_key(&request);
        if let Some(cached) = peek(self.storage, &key).await? {
            return Ok(cached);
        }
        let granted = match self.prompt.remote_permission(request).await {
            Ok(RemotePermissionResponse { granted }) => granted,
            Err(_) => false,
        };
        let decision = if granted {
            Decision::Granted
        } else {
            Decision::Denied
        };
        self.storage.write(key, decision.encode()).await?;
        Ok(decision)
    }
}

async fn peek(storage: &dyn Storage, key: &str) -> Result<Option<Decision>, StorageError> {
    let Some(raw) = storage.read(key.to_string()).await? else {
        return Ok(None);
    };
    Ok(Decision::decode(&mut &*raw).ok())
}

/// Canonical storage key for a device permission. The slug is human-readable
/// so a host developer inspecting storage can tell what's there.
pub fn device_storage_key(permission: &HostDevicePermissionRequest) -> String {
    format!("{PERMISSION_KEY_PREFIX}device:{}", device_slug(permission))
}

/// Canonical storage key for a remote permission bundle. Permissions inside
/// the bundle are sorted so equivalent batches (same set, different order)
/// collapse to one storage entry.
pub fn remote_storage_key(request: &RemotePermissionRequest) -> String {
    let mut slugs: Vec<String> = request.permissions.iter().map(remote_slug).collect();
    slugs.sort();
    format!("{PERMISSION_KEY_PREFIX}remote:{}", slugs.join("|"))
}

fn device_slug(permission: &HostDevicePermissionRequest) -> &'static str {
    match permission {
        HostDevicePermissionRequest::Notifications => "notifications",
        HostDevicePermissionRequest::Camera => "camera",
        HostDevicePermissionRequest::Microphone => "microphone",
        HostDevicePermissionRequest::Bluetooth => "bluetooth",
        HostDevicePermissionRequest::NFC => "nfc",
        HostDevicePermissionRequest::Location => "location",
        HostDevicePermissionRequest::Clipboard => "clipboard",
        HostDevicePermissionRequest::OpenUrl => "open-url",
        HostDevicePermissionRequest::Biometrics => "biometrics",
    }
}

fn remote_slug(permission: &RemotePermission) -> String {
    match permission {
        RemotePermission::Remote { domains } => {
            let mut sorted = domains.clone();
            sorted.sort();
            format!("domains:{}", sorted.join(","))
        }
        RemotePermission::WebRtc => "web-rtc".to_string(),
        RemotePermission::ChainSubmit => "chain-submit".to_string(),
        RemotePermission::PreimageSubmit => "preimage-submit".to_string(),
        RemotePermission::StatementSubmit => "statement-submit".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::lock::Mutex;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use truapi::v01;
    use truapi::v01::GenericError;
    use truapi_platform::{StorageKey, StorageValue};

    #[derive(Default)]
    struct MemStorage {
        inner: Mutex<HashMap<StorageKey, StorageValue>>,
    }

    #[async_trait]
    impl Storage for MemStorage {
        async fn read(&self, key: StorageKey) -> Result<Option<StorageValue>, StorageError> {
            Ok(self.inner.lock().await.get(&key).cloned())
        }
        async fn write(&self, key: StorageKey, value: StorageValue) -> Result<(), StorageError> {
            self.inner.lock().await.insert(key, value);
            Ok(())
        }
        async fn clear(&self, key: StorageKey) -> Result<(), StorageError> {
            self.inner.lock().await.remove(&key);
            Ok(())
        }
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

    #[async_trait]
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
    fn storage_key_is_stable_per_variant() {
        assert_eq!(
            device_storage_key(&HostDevicePermissionRequest::Camera),
            "truapi:permissions:device:camera",
        );
        let chain = RemotePermissionRequest {
            permissions: vec![RemotePermission::ChainSubmit],
        };
        assert_eq!(
            remote_storage_key(&chain),
            "truapi:permissions:remote:chain-submit",
        );
        let domains = RemotePermissionRequest {
            permissions: vec![RemotePermission::Remote {
                domains: vec!["b.example.com".into(), "a.example.com".into()],
            }],
        };
        assert_eq!(
            remote_storage_key(&domains),
            "truapi:permissions:remote:domains:a.example.com,b.example.com",
        );
    }

    #[test]
    fn remote_storage_key_sorts_bundle() {
        let a = RemotePermissionRequest {
            permissions: vec![RemotePermission::WebRtc, RemotePermission::ChainSubmit],
        };
        let b = RemotePermissionRequest {
            permissions: vec![RemotePermission::ChainSubmit, RemotePermission::WebRtc],
        };
        assert_eq!(remote_storage_key(&a), remote_storage_key(&b));
    }

    #[test]
    fn check_or_prompt_device_caches_grant() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt);

        let first = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let second = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        assert_eq!(first, Decision::Granted);
        assert_eq!(second, Decision::Granted);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn check_or_prompt_remote_caches_denial() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![], vec![false]);
        let service = PermissionsService::new(&storage, &prompt);

        let request = RemotePermissionRequest {
            permissions: vec![RemotePermission::ChainSubmit],
        };
        let first =
            futures::executor::block_on(service.check_or_prompt_remote(request.clone())).unwrap();
        let second = futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap();

        assert_eq!(first, Decision::Denied);
        assert_eq!(second, Decision::Denied);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn device_and_remote_caches_are_independent() {
        let storage = MemStorage::default();
        // Device denies, remote grants. If the caches collided we'd see the
        // same answer on the second call.
        let prompt = ScriptedPrompt::new(vec![false], vec![true]);
        let service = PermissionsService::new(&storage, &prompt);

        let device = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        let remote =
            futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
                permissions: vec![RemotePermission::ChainSubmit],
            }))
            .unwrap();

        assert_eq!(device, Decision::Denied);
        assert_eq!(remote, Decision::Granted);
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn device_prompt_does_not_invoke_remote_callback() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt);

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
        let service = PermissionsService::new(&storage, &prompt);

        let _ =
            futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
                permissions: vec![RemotePermission::WebRtc],
            }))
            .unwrap();
        assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
        assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn peek_returns_none_until_decided() {
        let storage = MemStorage::default();
        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt);

        let before =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(before, None);

        futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();

        let after =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(after, Some(Decision::Granted));
    }
}
