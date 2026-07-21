//! `Platform` implementation for the headless hosts.
//!
//! In-memory product and core storage, a WebSocket chain provider pointed at
//! the real People-chain statement store, and a [`UserConfirmation`] that
//! either auto-accepts or prompts on the CLI (the web/iOS "sign?" modal).
//! Auth-state transitions are published on a channel so the CLI can print the
//! pairing deeplink and observe connection status.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use truapi::v01;
use truapi_platform::{
    AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features, JsonRpcConnection, Navigation,
    Notifications, Permissions, PreimageHost, ProductStorage, ThemeHost, UserConfirmation,
    UserConfirmationReview,
};

use crate::chain::WsChainProvider;

/// How the host answers confirmation prompts (the web/iOS "sign?" modals).
#[derive(Clone, Copy)]
pub enum ApprovalPolicy {
    /// Approve every sensitive action without prompting (`--auto-accept`).
    AutoAccept,
    /// Prompt on the CLI (y/n) for every sensitive action.
    Prompt,
}

/// Headless-host platform shared by both roles.
pub struct CliPlatform {
    chain: WsChainProvider,
    product_storage: Mutex<HashMap<String, Vec<u8>>>,
    core_storage: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    product_storage_path: Option<PathBuf>,
    core_storage_path: Option<PathBuf>,
    preimages: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    approval: ApprovalPolicy,
    /// Serializes interactive CLI prompts so concurrent confirmations don't
    /// interleave on stdin.
    prompt_lock: AsyncMutex<()>,
}

impl CliPlatform {
    /// Build a platform whose chain provider connects to the network's People
    /// chain and whose optional state directory backs product/core storage.
    pub fn new(
        statement_store_url: impl Into<String>,
        live_chain_endpoints: &[crate::network::ChainEndpoint],
        storage_dir: Option<PathBuf>,
        approval: ApprovalPolicy,
    ) -> Arc<Self> {
        let (product_storage_path, core_storage_path) = storage_dir
            .as_ref()
            .map(|dir| {
                let _ = fs::create_dir_all(dir);
                (
                    Some(dir.join("product-storage.json")),
                    Some(dir.join("core-storage.json")),
                )
            })
            .unwrap_or((None, None));
        let product_storage = product_storage_path
            .as_deref()
            .map(load_string_map)
            .unwrap_or_default();
        let core_storage = core_storage_path
            .as_deref()
            .map(load_hex_key_map)
            .unwrap_or_default();

        Arc::new(Self {
            chain: WsChainProvider::new(statement_store_url, live_chain_endpoints),
            product_storage: Mutex::new(product_storage),
            core_storage: Mutex::new(core_storage),
            product_storage_path,
            core_storage_path,
            preimages: Mutex::new(HashMap::new()),
            approval,
            prompt_lock: AsyncMutex::new(()),
        })
    }

    fn core_key(key: &CoreStorageKey) -> Vec<u8> {
        use parity_scale_codec::Encode;
        key.encode()
    }

    fn persist_product_storage(&self) -> Result<(), String> {
        let Some(path) = &self.product_storage_path else {
            return Ok(());
        };
        let storage = self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned");
        save_string_map(path, &storage)
    }

    fn persist_core_storage(&self) -> Result<(), String> {
        let Some(path) = &self.core_storage_path else {
            return Ok(());
        };
        let storage = self
            .core_storage
            .lock()
            .expect("core storage mutex poisoned");
        save_hex_key_map(path, &storage)
    }

    /// Resolve a confirmation: auto-accept, or prompt y/n on the CLI.
    async fn decide(&self, action: &str, detail: String) -> bool {
        match self.approval {
            ApprovalPolicy::AutoAccept => {
                eprintln!("[auto-accept] {action}: {detail}");
                true
            }
            ApprovalPolicy::Prompt => {
                let _guard = self.prompt_lock.lock().await;
                prompt_yes_no(action, &detail).await
            }
        }
    }
}

/// Print a confirmation and read a y/n answer from the CLI (default: no).
async fn prompt_yes_no(action: &str, detail: &str) -> bool {
    let mut stdout = tokio::io::stdout();
    let _ = stdout
        .write_all(
            format!(
                "\n\u{2500}\u{2500} confirm: {action} \u{2500}\u{2500}\n{detail}\nApprove? [y/N] "
            )
            .as_bytes(),
        )
        .await;
    let _ = stdout.flush().await;
    let mut line = String::new();
    let mut reader = BufReader::new(tokio::io::stdin());
    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[async_trait]
impl ProductStorage for CliPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        Ok(self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned")
            .get(&key)
            .cloned())
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        {
            self.product_storage
                .lock()
                .expect("product storage mutex poisoned")
                .insert(key, value);
        }
        self.persist_product_storage()
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        {
            self.product_storage
                .lock()
                .expect("product storage mutex poisoned")
                .remove(&key);
        }
        self.persist_product_storage()
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
    }
}

#[async_trait]
impl CoreStorage for CliPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(self
            .core_storage
            .lock()
            .expect("core storage mutex poisoned")
            .get(&Self::core_key(&key))
            .cloned())
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        {
            self.core_storage
                .lock()
                .expect("core storage mutex poisoned")
                .insert(Self::core_key(&key), value);
        }
        self.persist_core_storage()
            .map_err(|reason| v01::GenericError { reason })
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        {
            self.core_storage
                .lock()
                .expect("core storage mutex poisoned")
                .remove(&Self::core_key(&key));
        }
        self.persist_core_storage()
            .map_err(|reason| v01::GenericError { reason })
    }
}

#[async_trait]
impl ChainProvider for CliPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        self.chain.connect(genesis_hash).await
    }
}

#[async_trait]
impl Navigation for CliPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        tracing::info!(%url, "navigate_to");
        Ok(())
    }
}

#[async_trait]
impl Notifications for CliPlatform {
    async fn push_notification(
        &self,
        _notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        Ok(v01::HostPushNotificationResponse { id: 1 })
    }
}

#[async_trait]
impl Permissions for CliPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        let granted = self
            .decide("device permission", format!("{request:?}"))
            .await;
        Ok(v01::HostDevicePermissionResponse { granted })
    }

    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        let granted = self
            .decide("remote permission", format!("{request:?}"))
            .await;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

#[async_trait]
impl Features for CliPlatform {
    async fn feature_supported(
        &self,
        _request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        Ok(v01::HostFeatureSupportedResponse { supported: true })
    }
}

impl truapi_platform::AuthPresenter for CliPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        // Machine-readable lines for orchestrators to observe pairing.
        match &state {
            AuthState::Pairing { deeplink } => println!("PAIRING_DEEPLINK {deeplink}"),
            AuthState::Authenticating => println!("PAIRING_AUTHENTICATING"),
            AuthState::Connected(_) => println!("PAIRING_CONNECTED"),
            AuthState::Disconnected => println!("PAIRING_DISCONNECTED"),
            AuthState::LoginFailed { reason } => println!("PAIRING_FAILED {reason}"),
        }
    }
}

#[async_trait]
impl UserConfirmation for CliPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        Ok(self.decide("sign request", format!("{review:?}")).await)
    }
}

impl ThemeHost for CliPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        Box::pin(stream::once(async { Ok(v01::ThemeVariant::Dark) }))
    }
}

impl PreimageHost for CliPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let value = self
            .preimages
            .lock()
            .expect("preimage mutex poisoned")
            .get(&key)
            .cloned();
        Box::pin(stream::once(async move { Ok(value) }))
    }
}

#[derive(Serialize, Deserialize)]
struct JsonMap {
    values: HashMap<String, String>,
}

fn load_string_map(path: &Path) -> HashMap<String, Vec<u8>> {
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str::<JsonMap>(&text)
        .ok()
        .map(|json| {
            json.values
                .into_iter()
                .filter_map(|(key, value)| hex::decode(value).ok().map(|bytes| (key, bytes)))
                .collect()
        })
        .unwrap_or_default()
}

fn save_string_map(path: &Path, values: &HashMap<String, Vec<u8>>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create storage dir: {err}"))?;
    }
    let json = JsonMap {
        values: values
            .iter()
            .map(|(key, value)| (key.clone(), hex::encode(value)))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&json).map_err(|err| err.to_string())?;
    fs::write(path, text).map_err(|err| format!("write {}: {err}", path.display()))
}

fn load_hex_key_map(path: &Path) -> HashMap<Vec<u8>, Vec<u8>> {
    load_string_map(path)
        .into_iter()
        .filter_map(|(key, value)| hex::decode(key).ok().map(|decoded| (decoded, value)))
        .collect()
}

fn save_hex_key_map(path: &Path, values: &HashMap<Vec<u8>, Vec<u8>>) -> Result<(), String> {
    let keyed: HashMap<String, Vec<u8>> = values
        .iter()
        .map(|(key, value)| (hex::encode(key), value.clone()))
        .collect();
    save_string_map(path, &keyed)
}
