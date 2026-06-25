//! Permission state machine (ask -> granted | denied), backed by the platform
//! [`CoreStorage`] trait with a reserved `truapi:permissions:` key prefix.
//!
//! The v0.1 wire protocol keeps device permissions (camera, mic, NFC, ...)
//! separate from remote permissions (domain access, chain submit, ...), so
//! this module exposes two `check_or_prompt` entrypoints that route to the
//! matching platform callback. The cache layer is shared but keys live in
//! distinct sub-namespaces so a device grant cannot authorize a remote
//! operation by accident.

use parity_scale_codec::{Decode, Encode};

use truapi::v01;
use truapi::v01::{
    HostDevicePermissionRequest, HostDevicePermissionResponse, RemotePermission,
    RemotePermissionRequest, RemotePermissionResponse,
};
use truapi_platform::{CoreStorage, CoreStorageKey, Permissions};

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
/// short-circuit. Generic over the concrete `CoreStorage` + `Permissions` impls
/// so callers (e.g. `PlatformRuntimeHost<P>`) can stay non-`dyn`.
pub struct PermissionsService<'a, S: CoreStorage, P: Permissions> {
    storage: &'a S,
    prompt: &'a P,
}

impl<'a, S: CoreStorage, P: Permissions> PermissionsService<'a, S, P> {
    /// Construct a service backed by the given storage + prompt callbacks.
    pub fn new(storage: &'a S, prompt: &'a P) -> Self {
        Self { storage, prompt }
    }

    /// Returns the stored decision for a device permission without prompting.
    pub async fn peek_device(
        &self,
        permission: &HostDevicePermissionRequest,
    ) -> Result<Option<Decision>, v01::GenericError> {
        peek(self.storage, &device_storage_key(permission)).await
    }

    /// Returns the stored decision for a remote permission without
    /// prompting.
    pub async fn peek_remote(
        &self,
        request: &RemotePermissionRequest,
    ) -> Result<Option<Decision>, v01::GenericError> {
        peek(self.storage, &remote_storage_key(request)).await
    }

    /// Returns the cached device decision if any, otherwise prompts the
    /// platform's `device_permission` callback and persists the answer.
    pub async fn check_or_prompt_device(
        &self,
        permission: HostDevicePermissionRequest,
    ) -> Result<Decision, v01::GenericError> {
        let key = device_storage_key(&permission);
        if let Some(cached) = peek(self.storage, &key).await? {
            return Ok(cached);
        }
        // Only a genuine user decision is persisted. A prompt-callback error is
        // transient (UI unavailable, IPC timeout), not a denial, so fail closed
        // for this call but do not cache it — the next request re-prompts rather
        // than locking the capability out permanently with no revoke path.
        let granted = match self.prompt.device_permission(permission).await {
            Ok(HostDevicePermissionResponse { granted }) => granted,
            Err(_) => return Ok(Decision::Denied),
        };
        let decision = if granted {
            Decision::Granted
        } else {
            Decision::Denied
        };
        self.storage
            .write_core_storage(
                CoreStorageKey::PermissionDecision { storage_key: key },
                decision.encode(),
            )
            .await?;
        Ok(decision)
    }

    /// Returns the cached remote decision if any, otherwise prompts the
    /// platform's `remote_permission` callback and persists the answer.
    pub async fn check_or_prompt_remote(
        &self,
        request: RemotePermissionRequest,
    ) -> Result<Decision, v01::GenericError> {
        let key = remote_storage_key(&request);
        if let Some(cached) = peek(self.storage, &key).await? {
            return Ok(cached);
        }
        // See `check_or_prompt_device`: persist only a genuine user decision; a
        // transient callback error fails closed for this call without caching.
        let granted = match self.prompt.remote_permission(request).await {
            Ok(RemotePermissionResponse { granted }) => granted,
            Err(_) => return Ok(Decision::Denied),
        };
        let decision = if granted {
            Decision::Granted
        } else {
            Decision::Denied
        };
        self.storage
            .write_core_storage(
                CoreStorageKey::PermissionDecision { storage_key: key },
                decision.encode(),
            )
            .await?;
        Ok(decision)
    }
}

async fn peek<S: CoreStorage>(
    storage: &S,
    key: &str,
) -> Result<Option<Decision>, v01::GenericError> {
    let Some(raw) = storage
        .read_core_storage(CoreStorageKey::PermissionDecision {
            storage_key: key.to_string(),
        })
        .await?
    else {
        return Ok(None);
    };
    Ok(Decision::decode(&mut &*raw).ok())
}

/// Canonical storage key for a device permission. The slug is human-readable
/// so a host developer inspecting storage can tell what's there.
pub fn device_storage_key(permission: &HostDevicePermissionRequest) -> String {
    format!("{PERMISSION_KEY_PREFIX}device:{}", device_slug(permission))
}

/// Canonical storage key for a remote permission. The permission is
/// canonicalized (domain lists lowercased, sorted, and de-duplicated) then
/// SCALE-encoded and hex-encoded so attacker-controlled domain strings cannot
/// collide with another permission by injecting separator characters.
pub fn remote_storage_key(request: &RemotePermissionRequest) -> String {
    let canonical = canonical_remote_request(request);
    format!(
        "{PERMISSION_KEY_PREFIX}remote:{}",
        hex::encode(canonical.encode())
    )
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
        match key {
            CoreStorageKey::PermissionDecision { storage_key } => storage_key,
            CoreStorageKey::AuthSession => "auth-session".to_string(),
            CoreStorageKey::PairingDeviceIdentity => "pairing-device-identity".to_string(),
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
            permission: RemotePermission::ChainSubmit,
        };
        let expected = format!(
            "truapi:permissions:remote:{}",
            hex::encode(canonical_remote_request(&chain).encode())
        );
        assert_eq!(remote_storage_key(&chain), expected);

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
        assert_eq!(remote_storage_key(&unsorted), remote_storage_key(&sorted));
    }

    #[test]
    fn remote_storage_key_is_case_insensitive_and_dedups_domains() {
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
        assert_eq!(remote_storage_key(&mixed), remote_storage_key(&canonical));
    }

    #[test]
    fn remote_storage_key_handles_separator_chars_in_domains() {
        // Domain strings containing `|`, `,`, or the `truapi:permissions:`
        // prefix must not be able to forge a key that matches an unrelated
        // permission. We compare against a benign permission with the same
        // logical set of domains but no injection attempt.
        let injecting = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec![
                    "a|b".into(),
                    "c,d".into(),
                    "truapi:permissions:remote:web-rtc".into(),
                ],
            },
        };
        let benign_same_set = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec!["x".into(), "y".into(), "z".into()],
            },
        };
        let injecting_key = remote_storage_key(&injecting);
        let benign_key = remote_storage_key(&benign_same_set);
        assert_ne!(injecting_key, benign_key);

        // The injecting permission must also be distinct from the `WebRtc`
        // variant it tries to impersonate via crafted strings.
        let webrtc = RemotePermissionRequest {
            permission: RemotePermission::WebRtc,
        };
        assert_ne!(injecting_key, remote_storage_key(&webrtc));

        // Re-ordering the same domains still collapses to a single key
        // (canonicalization is order-independent).
        let injecting_reordered = RemotePermissionRequest {
            permission: RemotePermission::Remote {
                domains: vec![
                    "truapi:permissions:remote:web-rtc".into(),
                    "c,d".into(),
                    "a|b".into(),
                ],
            },
        };
        assert_eq!(injecting_key, remote_storage_key(&injecting_reordered));
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
            permission: RemotePermission::ChainSubmit,
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
                permission: RemotePermission::ChainSubmit,
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
                permission: RemotePermission::WebRtc,
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

    /// Prompt callback that always errors, to exercise the transient-failure
    /// path (fail closed for the current call, but do not persist the error).
    struct FailingPrompt;

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
        let service = PermissionsService::new(&storage, &prompt);

        let decision = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .unwrap();
        assert_eq!(decision, Decision::Denied);

        // A transient callback error is fail-closed for this call but NOT
        // cached, so peek still sees no decision and the next request
        // re-prompts rather than permanently locking out the capability.
        let cached =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(
            cached, None,
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
            CoreStorageKey::PermissionDecision {
                storage_key: device_storage_key(&HostDevicePermissionRequest::Camera),
            },
            vec![0xff, 0xfe, 0xfd],
        ))
        .unwrap();

        let prompt = ScriptedPrompt::new(vec![true], vec![]);
        let service = PermissionsService::new(&storage, &prompt);

        let peeked =
            futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
                .unwrap();
        assert_eq!(peeked, None, "corrupt entry must decode as absent");
    }

    /// Storage failures must propagate to the caller; the service must not
    /// swallow them by silently returning a default Decision.
    #[derive(Default)]
    struct FailingStorage;

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
        let service = PermissionsService::new(&storage, &prompt);

        let err = futures::executor::block_on(
            service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
        )
        .expect_err("read failure must surface");
        assert!(matches!(err, v01::GenericError { .. }));
    }
}
