//! `Platform` implementation for the headless hosts.
//!
//! In-memory product and core storage, a WebSocket chain provider pointed at
//! the real People-chain statement store, and a [`UserConfirmation`] that
//! either auto-accepts or prompts on the CLI (the web/iOS "sign?" modal).
//! Auth-state transitions are published on a channel so the CLI can print the
//! pairing deeplink and observe connection status.

use std::collections::HashMap;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use truapi::latest as api;
use truapi_platform::{
    AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features, JsonRpcConnection, Navigation,
    Notifications, Permissions, PreimageHost, ProductStorage, ProductStorageKey, SessionUiInfo,
    ThemeHost, UserConfirmation, UserConfirmationReview,
};

use crate::chain::WsChainProvider;
use crate::terminal_ui::{SystemEvent, UiHandle};

static NEXT_STORAGE_TEMP_ID: AtomicU32 = AtomicU32::new(0);

/// How the host answers confirmation prompts (the web/iOS "sign?" modals).
#[derive(Clone, Copy)]
pub enum ApprovalPolicy {
    /// Approve every sensitive action without prompting (`--auto-accept`).
    AutoAccept,
    /// Prompt on the CLI (y/n) for every sensitive action.
    Prompt,
}

/// Filesystem locations for one host/session runtime.
#[derive(Clone)]
pub struct CliStoragePaths {
    state_dir: PathBuf,
    product_storage_dir: PathBuf,
    pairing_scope: Option<PairingStorageScope>,
}

#[derive(Clone)]
struct PairingStorageScope {
    network_dir: PathBuf,
    bootstrap_dir: PathBuf,
}

impl CliStoragePaths {
    pub fn new(state_dir: PathBuf, product_storage_dir: PathBuf) -> Self {
        Self {
            state_dir,
            product_storage_dir,
            pairing_scope: None,
        }
    }

    /// Resolve the last paired user, falling back to a role-level bootstrap
    /// directory until the first identity is known.
    pub fn pairing(network_dir: PathBuf) -> Self {
        let bootstrap_dir = network_dir.join("pairing-host");
        let state_dir = read_current_pairing_user(&bootstrap_dir)
            .map(|user_id| network_dir.join(format!("{user_id}_pairing_host")))
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| bootstrap_dir.clone());
        let product_storage_dir = if state_dir == bootstrap_dir
            && bootstrap_dir.join("storage").join("default").is_dir()
        {
            bootstrap_dir.join("storage").join("default")
        } else {
            state_dir.join("storage")
        };
        Self {
            product_storage_dir,
            state_dir,
            pairing_scope: Some(PairingStorageScope {
                network_dir,
                bootstrap_dir,
            }),
        }
    }
}

/// Headless-host platform shared by both roles.
pub struct CliPlatform {
    chain: WsChainProvider,
    product_storage: Mutex<HashMap<String, HashMap<String, Vec<u8>>>>,
    core_storage: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    product_storage_dir: Mutex<Option<PathBuf>>,
    core_storage_path: Mutex<Option<PathBuf>>,
    state_dir: Mutex<Option<PathBuf>>,
    pairing_scope: Option<PairingStorageScope>,
    preimages: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    next_notification_id: AtomicU32,
    scheduled_notifications:
        Arc<Mutex<HashMap<api::NotificationId, api::HostPushNotificationRequest>>>,
    approval: ApprovalPolicy,
    ui: Option<UiHandle>,
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
        storage: Option<CliStoragePaths>,
        approval: ApprovalPolicy,
        ui: Option<UiHandle>,
    ) -> Arc<Self> {
        let (product_storage_dir, legacy_product_storage_path, core_storage_path) = storage
            .as_ref()
            .map(|paths| {
                if let Err(err) = fs::create_dir_all(&paths.state_dir) {
                    tracing::warn!(
                        path = %paths.state_dir.display(),
                        %err,
                        "could not create CLI storage dir"
                    );
                }
                (
                    Some(paths.product_storage_dir.clone()),
                    Some(paths.state_dir.join("product-storage.json")),
                    Some(paths.state_dir.join("core-storage.json")),
                )
            })
            .unwrap_or((None, None, None));
        let product_storage = product_storage_dir
            .as_deref()
            .zip(legacy_product_storage_path.as_deref())
            .map(|(directory, legacy)| load_product_storage(directory, legacy))
            .unwrap_or_default();
        let core_storage = core_storage_path
            .as_deref()
            .map(load_hex_key_map)
            .unwrap_or_default();

        Arc::new(Self {
            chain: WsChainProvider::new(statement_store_url, live_chain_endpoints),
            product_storage: Mutex::new(product_storage),
            core_storage: Mutex::new(core_storage),
            product_storage_dir: Mutex::new(product_storage_dir),
            core_storage_path: Mutex::new(core_storage_path),
            state_dir: Mutex::new(storage.as_ref().map(|paths| paths.state_dir.clone())),
            pairing_scope: storage.and_then(|paths| paths.pairing_scope),
            preimages: Mutex::new(HashMap::new()),
            next_notification_id: AtomicU32::new(1),
            scheduled_notifications: Arc::new(Mutex::new(HashMap::new())),
            approval,
            ui,
            prompt_lock: AsyncMutex::new(()),
        })
    }

    fn core_key(key: &CoreStorageKey) -> Vec<u8> {
        use parity_scale_codec::Encode;
        key.encode()
    }

    fn persist_product_storage(
        &self,
        product_id: &str,
        values: &HashMap<String, Vec<u8>>,
    ) -> Result<(), String> {
        let Some(directory) = self
            .product_storage_dir
            .lock()
            .expect("product storage path mutex poisoned")
            .clone()
        else {
            return Ok(());
        };
        save_product_storage(&directory, product_id, values)
    }

    fn persist_core_storage(&self) -> Result<(), String> {
        let Some(path) = self
            .core_storage_path
            .lock()
            .expect("core storage path mutex poisoned")
            .clone()
        else {
            return Ok(());
        };
        let storage = self
            .core_storage
            .lock()
            .expect("core storage mutex poisoned");
        save_hex_key_map(&path, &storage)
    }

    /// Current identity-owned state directory (or the pairing bootstrap before
    /// a user has connected).
    pub fn state_dir(&self) -> Option<PathBuf> {
        self.state_dir
            .lock()
            .expect("state path mutex poisoned")
            .clone()
    }

    fn switch_pairing_user_storage(&self, user_id: &str) -> Result<(), String> {
        let Some(scope) = &self.pairing_scope else {
            return Ok(());
        };
        crate::sessions::validate_name(user_id)?;
        let target_state = scope.network_dir.join(format!("{user_id}_pairing_host"));
        let current_state = self
            .state_dir
            .lock()
            .expect("state path mutex poisoned")
            .clone();
        if current_state.as_ref() == Some(&target_state) {
            persist_current_pairing_user(&scope.bootstrap_dir, user_id)?;
            return Ok(());
        }

        fs::create_dir_all(&target_state)
            .map_err(|error| format!("create {}: {error}", target_state.display()))?;
        let target_product_dir = target_state.join("storage");
        let target_core_path = target_state.join("core-storage.json");
        let migrating_bootstrap = current_state.as_ref() == Some(&scope.bootstrap_dir);

        // A fresh login writes these values before its username is known.
        // Carry only that pairing bootstrap across namespaces; permissions,
        // allowances, and product KV remain isolated to their previous user.
        let transient_keys = [
            CoreStorageKey::AuthSession,
            CoreStorageKey::PairingDeviceIdentity,
            CoreStorageKey::LastProcessedPairingStatement,
        ]
        .map(|key| Self::core_key(&key));
        let carried = {
            // Keep the same path -> storage lock order used by persistence so
            // an auth transition cannot deadlock with a concurrent core write.
            let current_path = self
                .core_storage_path
                .lock()
                .expect("core storage path mutex poisoned");
            let mut current = self
                .core_storage
                .lock()
                .expect("core storage mutex poisoned");
            if migrating_bootstrap {
                current.drain().collect::<Vec<_>>()
            } else {
                let carried = transient_keys
                    .iter()
                    .filter_map(|key| current.remove(key).map(|value| (key.clone(), value)))
                    .collect::<Vec<_>>();
                if let Some(path) = current_path.as_deref() {
                    save_hex_key_map(path, &current)?;
                }
                carried
            }
        };

        let mut target_core = load_hex_key_map(&target_core_path);
        target_core.extend(carried);
        let mut target_products = load_product_storage(
            &target_product_dir,
            &target_state.join("product-storage.json"),
        );
        if migrating_bootstrap {
            target_products.extend(
                self.product_storage
                    .lock()
                    .expect("product storage mutex poisoned")
                    .clone(),
            );
            for (product_id, values) in &target_products {
                save_product_storage(&target_product_dir, product_id, values)?;
            }
        }
        {
            let mut core_storage_path = self
                .core_storage_path
                .lock()
                .expect("core storage path mutex poisoned");
            let mut core_storage = self
                .core_storage
                .lock()
                .expect("core storage mutex poisoned");
            *core_storage = target_core;
            *core_storage_path = Some(target_core_path);
        }
        *self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned") = target_products;
        *self
            .product_storage_dir
            .lock()
            .expect("product storage path mutex poisoned") = Some(target_product_dir);
        *self.state_dir.lock().expect("state path mutex poisoned") = Some(target_state);
        self.persist_core_storage()?;
        persist_current_pairing_user(&scope.bootstrap_dir, user_id)
    }

    /// Resolve a confirmation: auto-accept, or prompt y/n on the CLI.
    async fn decide(&self, action: &str, detail: String) -> bool {
        match self.approval {
            ApprovalPolicy::AutoAccept => {
                if let Some(ui) = &self.ui {
                    ui.success(format!("Approved {action} automatically"), Some(detail));
                } else {
                    crate::terminal_ui::output_success(
                        format!("Approved {action} automatically"),
                        Some(detail),
                    );
                }
                true
            }
            ApprovalPolicy::Prompt => {
                let _guard = self.prompt_lock.lock().await;
                if let Some(ui) = &self.ui {
                    ui.confirm(action, detail).await
                } else {
                    prompt_yes_no(action, &detail).await
                }
            }
        }
    }
}

/// Print a confirmation and read a y/n answer from the CLI (default: no).
async fn prompt_yes_no(action: &str, detail: &str) -> bool {
    if !std::io::stdin().is_terminal() {
        eprintln!("approval required for {action}, but stdin is not a terminal; rejecting");
        return false;
    }
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
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, api::HostLocalStorageReadError> {
        let scoped = ProductStorageKey::decode(&key)
            .map_err(|reason| api::HostLocalStorageReadError::Unknown { reason })?;
        Ok(self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned")
            .get(scoped.product_id())
            .and_then(|values| values.get(scoped.key()))
            .cloned())
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), api::HostLocalStorageReadError> {
        let scoped = ProductStorageKey::decode(&key)
            .map_err(|reason| api::HostLocalStorageReadError::Unknown { reason })?;
        let mut storage = self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned");
        let values = storage.entry(scoped.product_id().to_string()).or_default();
        values.insert(scoped.key().to_string(), value);
        self.persist_product_storage(scoped.product_id(), values)
            .map_err(|reason| api::HostLocalStorageReadError::Unknown { reason })
    }

    async fn clear(&self, key: String) -> Result<(), api::HostLocalStorageReadError> {
        let scoped = ProductStorageKey::decode(&key)
            .map_err(|reason| api::HostLocalStorageReadError::Unknown { reason })?;
        let mut storage = self
            .product_storage
            .lock()
            .expect("product storage mutex poisoned");
        let values = storage.entry(scoped.product_id().to_string()).or_default();
        values.remove(scoped.key());
        self.persist_product_storage(scoped.product_id(), values)
            .map_err(|reason| api::HostLocalStorageReadError::Unknown { reason })
    }
}

#[async_trait]
impl CoreStorage for CliPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, api::GenericError> {
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
    ) -> Result<(), api::GenericError> {
        {
            self.core_storage
                .lock()
                .expect("core storage mutex poisoned")
                .insert(Self::core_key(&key), value);
        }
        self.persist_core_storage()
            .map_err(|reason| api::GenericError { reason })
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), api::GenericError> {
        {
            self.core_storage
                .lock()
                .expect("core storage mutex poisoned")
                .remove(&Self::core_key(&key));
        }
        self.persist_core_storage()
            .map_err(|reason| api::GenericError { reason })
    }
}

#[async_trait]
impl ChainProvider for CliPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, api::GenericError> {
        self.chain.connect(genesis_hash).await
    }
}

#[async_trait]
impl Navigation for CliPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), api::HostNavigateToError> {
        tracing::debug!(%url, "navigate_to");
        Ok(())
    }
}

#[async_trait]
impl Notifications for CliPlatform {
    async fn push_notification(
        &self,
        notification: api::HostPushNotificationRequest,
    ) -> Result<api::HostPushNotificationResponse, api::GenericError> {
        let id = self.next_notification_id.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if let Some(scheduled_at) = notification.scheduled_at.filter(|at| *at > now) {
            {
                let mut pending = self
                    .scheduled_notifications
                    .lock()
                    .expect("notification mutex poisoned");
                if pending.len() >= 64 {
                    return Err(api::GenericError {
                        reason: "the CLI notification schedule is full (64 pending notifications)"
                            .to_string(),
                    });
                }
                pending.insert(id, notification.clone());
            }
            emit_notification_event(
                self.ui.as_ref(),
                SystemEvent::NotificationScheduled {
                    id,
                    text: notification.text.clone(),
                    scheduled_at,
                },
            );
            let pending = self.scheduled_notifications.clone();
            let ui = self.ui.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(scheduled_at.saturating_sub(now))).await;
                let notification = pending
                    .lock()
                    .expect("notification mutex poisoned")
                    .remove(&id);
                if let Some(notification) = notification {
                    emit_notification_event(
                        ui.as_ref(),
                        SystemEvent::NotificationDelivered {
                            id,
                            text: notification.text,
                            deeplink: notification.deeplink,
                        },
                    );
                }
            });
        } else {
            emit_notification_event(
                self.ui.as_ref(),
                SystemEvent::NotificationDelivered {
                    id,
                    text: notification.text,
                    deeplink: notification.deeplink,
                },
            );
        }
        Ok(api::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: api::NotificationId) -> Result<(), api::GenericError> {
        if self
            .scheduled_notifications
            .lock()
            .expect("notification mutex poisoned")
            .remove(&id)
            .is_some()
        {
            emit_notification_event(self.ui.as_ref(), SystemEvent::NotificationCancelled { id });
        }
        Ok(())
    }
}

fn emit_notification_event(ui: Option<&UiHandle>, event: SystemEvent) {
    if let Some(ui) = ui {
        ui.event(event);
    } else {
        crate::terminal_ui::output_event(event);
    }
}

#[async_trait]
impl Permissions for CliPlatform {
    async fn device_permission(
        &self,
        _request: api::HostDevicePermissionRequest,
    ) -> Result<api::HostDevicePermissionResponse, api::GenericError> {
        let granted = self
            .decide(
                "device permission",
                "A product requested access to a device capability.".to_string(),
            )
            .await;
        Ok(api::HostDevicePermissionResponse { granted })
    }

    async fn remote_permission(
        &self,
        _request: api::RemotePermissionRequest,
    ) -> Result<api::RemotePermissionResponse, api::GenericError> {
        let granted = self
            .decide(
                "remote permission",
                "A paired product requested a remote capability.".to_string(),
            )
            .await;
        Ok(api::RemotePermissionResponse { granted })
    }
}

#[async_trait]
impl Features for CliPlatform {
    async fn feature_supported(
        &self,
        _request: api::HostFeatureSupportedRequest,
    ) -> Result<api::HostFeatureSupportedResponse, api::GenericError> {
        Ok(api::HostFeatureSupportedResponse { supported: false })
    }
}

impl truapi_platform::AuthPresenter for CliPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        if let AuthState::Connected(info) = &state
            && let Some(user_id) = storage_user_id(info)
            && let Err(reason) = self.switch_pairing_user_storage(user_id)
        {
            tracing::warn!(%reason, %user_id, "could not switch pairing-host user storage");
        }
        let (connection, event) = match &state {
            AuthState::Pairing { deeplink } => (
                "pairing".to_string(),
                SystemEvent::PairingDeeplink {
                    url: deeplink.clone(),
                },
            ),
            AuthState::Authenticating => (
                "authenticating".to_string(),
                SystemEvent::PairingAuthenticating,
            ),
            AuthState::Connected(info) => (
                connected_user_id(info).unwrap_or("connected").to_string(),
                SystemEvent::PairingConnected {
                    user_id: connected_user_id(info).map(str::to_string),
                },
            ),
            AuthState::Disconnected => {
                ("disconnected".to_string(), SystemEvent::PairingDisconnected)
            }
            AuthState::LoginFailed { reason } => (
                "failed".to_string(),
                SystemEvent::PairingFailed {
                    reason: reason.clone(),
                },
            ),
        };
        if let Some(ui) = &self.ui {
            ui.connection(connection);
            ui.event(event);
        } else {
            crate::terminal_ui::output_event(event);
        }
    }
}

fn connected_user_id(info: &SessionUiInfo) -> Option<&str> {
    info.full_username
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            info.lite_username
                .as_deref()
                .filter(|value| !value.is_empty())
        })
}

fn storage_user_id(info: &SessionUiInfo) -> Option<&str> {
    info.lite_username
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            info.full_username
                .as_deref()
                .filter(|value| !value.is_empty())
        })
}

#[async_trait]
impl UserConfirmation for CliPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, api::GenericError> {
        let (action, detail) = approval_summary(&review);
        Ok(self.decide(action, detail).await)
    }
}

fn approval_summary(review: &UserConfirmationReview) -> (&'static str, String) {
    match review {
        UserConfirmationReview::SignPayload(_) => (
            "sign payload",
            "A product requested a SCALE payload signature.".to_string(),
        ),
        UserConfirmationReview::SignRaw(_) => (
            "sign raw data",
            "A product requested a raw-data signature. The payload is hidden here.".to_string(),
        ),
        UserConfirmationReview::StatementStoreProductSign(review) => (
            "sign statement proof",
            format!(
                "Product {} requested a Statement Store proof signature over a {}-byte payload.",
                review.account.dot_ns_identifier,
                review.payload.len()
            ),
        ),
        UserConfirmationReview::CreateTransaction(_) => (
            "create transaction",
            "A product requested a transaction from one of your accounts.".to_string(),
        ),
        UserConfirmationReview::AccountAlias(review) => (
            "derive account alias",
            format!(
                "Product {} requested a contextual account alias.",
                review.calling_product_id
            ),
        ),
        UserConfirmationReview::CreateProof(review) => (
            "create account proof",
            format!(
                "Product {} requested a contextual proof bound to {} bytes.",
                review.calling_product_id,
                review.message.len()
            ),
        ),
        UserConfirmationReview::IdentityDisclosure(review) => (
            "share identity",
            format!(
                "Product {} requested your primary identity.",
                review.product_id
            ),
        ),
        UserConfirmationReview::ResourceAllocation(_) => (
            "allocate resources",
            "A product requested host-managed resources.".to_string(),
        ),
        UserConfirmationReview::PreimageSubmit(review) => (
            "submit preimage",
            format!(
                "A product requested submission of a {}-byte preimage.",
                review.size
            ),
        ),
        UserConfirmationReview::AccountAccess(review) => (
            "access another product account",
            format!(
                "Product {} requested access to the {} account.",
                review.requesting_product_id, review.target_product_id
            ),
        ),
    }
}

impl ThemeHost for CliPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<api::ThemeVariant, api::GenericError>> {
        Box::pin(stream::once(async { Ok(api::ThemeVariant::Dark) }))
    }
}

impl PreimageHost for CliPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, api::GenericError>> {
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductStorageDocument {
    version: u32,
    product_id: String,
    values: HashMap<String, String>,
}

fn load_product_storage(
    directory: &Path,
    legacy_path: &Path,
) -> HashMap<String, HashMap<String, Vec<u8>>> {
    let legacy_exists = legacy_path.is_file();
    let mut migration_safe = true;
    let mut products = HashMap::<String, HashMap<String, Vec<u8>>>::new();

    if legacy_exists {
        match read_string_map(legacy_path) {
            Ok(values) => {
                for (key, value) in values {
                    match ProductStorageKey::decode(&key) {
                        Ok(scoped) => {
                            products
                                .entry(scoped.product_id().to_string())
                                .or_default()
                                .insert(scoped.key().to_string(), value);
                        }
                        Err(error) => {
                            migration_safe = false;
                            tracing::warn!(
                                path = %legacy_path.display(),
                                %error,
                                "could not migrate an unrecognized product storage key"
                            );
                        }
                    }
                }
            }
            Err(error) => {
                migration_safe = false;
                tracing::warn!(
                    path = %legacy_path.display(),
                    %error,
                    "could not decode legacy product storage"
                );
            }
        }
    }

    let entries = match fs::read_dir(directory) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(
                path = %directory.display(),
                %error,
                "could not list per-product CLI storage"
            );
            None
        }
    };
    if let Some(entries) = entries {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some((product_id, values)) = load_product_storage_file(&path) else {
                continue;
            };
            let expected_path = product_storage_path(directory, &product_id);
            if path != expected_path {
                tracing::warn!(
                    path = %path.display(),
                    expected = %expected_path.display(),
                    "ignored product storage with a non-canonical filename"
                );
                continue;
            }
            products.entry(product_id).or_default().extend(values);
        }
    }

    if legacy_exists && migration_safe {
        let migrated = products.iter().try_for_each(|(product_id, values)| {
            save_product_storage(directory, product_id, values)
        });
        match migrated {
            Ok(()) => {
                let backup = legacy_path.with_file_name("product-storage.v1.json.migrated");
                if backup.exists() {
                    tracing::warn!(
                        path = %legacy_path.display(),
                        backup = %backup.display(),
                        "legacy product storage was migrated but its backup path already exists"
                    );
                } else if let Err(error) = fs::rename(legacy_path, &backup) {
                    tracing::warn!(
                        path = %legacy_path.display(),
                        backup = %backup.display(),
                        %error,
                        "could not retain migrated product storage backup"
                    );
                }
            }
            Err(error) => tracing::warn!(
                path = %legacy_path.display(),
                %error,
                "could not migrate legacy product storage"
            ),
        }
    }

    products
}

fn load_product_storage_file(path: &Path) -> Option<(String, HashMap<String, Vec<u8>>)> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "could not read product storage");
            return None;
        }
    };
    let document = match serde_json::from_str::<ProductStorageDocument>(&text) {
        Ok(document) if document.version == 1 => document,
        Ok(document) => {
            tracing::warn!(
                path = %path.display(),
                version = document.version,
                "unsupported product storage version"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "could not decode product storage");
            return None;
        }
    };
    let normalized = match ProductStorageKey::new(&document.product_id, "") {
        Ok(scoped) => scoped.product_id().to_string(),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "product storage contains an invalid product id"
            );
            return None;
        }
    };
    let values = document
        .values
        .into_iter()
        .map(|(key, value)| hex::decode(value).map(|bytes| (key, bytes)))
        .collect::<Result<HashMap<_, _>, _>>();
    let values = match values {
        Ok(values) => values,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "product storage contains an invalid value"
            );
            return None;
        }
    };
    Some((normalized, values))
}

fn save_product_storage(
    directory: &Path,
    product_id: &str,
    values: &HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "create product storage directory {}: {error}",
            directory.display()
        )
    })?;
    let document = ProductStorageDocument {
        version: 1,
        product_id: product_id.to_string(),
        values: values
            .iter()
            .map(|(key, value)| (key.clone(), hex::encode(value)))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&document).map_err(|error| error.to_string())?;
    atomic_write(
        &product_storage_path(directory, product_id),
        text.as_bytes(),
    )
}

fn product_storage_path(directory: &Path, product_id: &str) -> PathBuf {
    let mut slug = product_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-') {
                character
            } else {
                '-'
            }
        })
        .take(48)
        .collect::<String>();
    slug = slug
        .trim_matches(|character| matches!(character, '.' | '-'))
        .to_string();
    if slug.is_empty() {
        slug.push_str("product");
    }
    let digest = Sha256::digest(product_id.as_bytes());
    directory.join(format!("{slug}--{}.json", hex::encode(digest)))
}

fn load_string_map(path: &Path) -> HashMap<String, Vec<u8>> {
    match read_string_map(path) {
        Ok(values) => values,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not read CLI storage");
            HashMap::new()
        }
    }
}

fn read_string_map(path: &Path) -> std::io::Result<HashMap<String, Vec<u8>>> {
    let text = fs::read_to_string(path)?;
    let json = serde_json::from_str::<JsonMap>(&text)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    json.values
        .into_iter()
        .map(|(key, value)| {
            hex::decode(value)
                .map(|bytes| (key, bytes))
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
        .collect()
}

fn save_string_map(path: &Path, values: &HashMap<String, Vec<u8>>) -> Result<(), String> {
    let json = JsonMap {
        values: values
            .iter()
            .map(|(key, value)| (key.clone(), hex::encode(value)))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&json).map_err(|err| err.to_string())?;
    atomic_write(path, text.as_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("storage path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create storage dir: {error}"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("storage.json");
    let temporary_id = NEXT_STORAGE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary =
        path.with_file_name(format!(".{name}.{}.{temporary_id}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync {}: {error}", temporary.display()))?;
    drop(file);
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("replace {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, path).map_err(|error| format!("persist {}: {error}", path.display()))?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync storage dir {}: {error}", parent.display()))?;
    Ok(())
}

fn load_hex_key_map(path: &Path) -> HashMap<Vec<u8>, Vec<u8>> {
    load_string_map(path)
        .into_iter()
        .filter_map(|(key, value)| hex::decode(key).ok().map(|decoded| (decoded, value)))
        .collect()
}

const CURRENT_PAIRING_USER_FILE: &str = "current-user";

fn read_current_pairing_user(bootstrap_dir: &Path) -> Option<String> {
    let user_id = fs::read_to_string(bootstrap_dir.join(CURRENT_PAIRING_USER_FILE))
        .ok()?
        .trim()
        .to_string();
    crate::sessions::validate_name(&user_id).ok()?;
    Some(user_id)
}

fn persist_current_pairing_user(bootstrap_dir: &Path, user_id: &str) -> Result<(), String> {
    fs::create_dir_all(bootstrap_dir)
        .map_err(|error| format!("create {}: {error}", bootstrap_dir.display()))?;
    let path = bootstrap_dir.join(CURRENT_PAIRING_USER_FILE);
    let temporary = bootstrap_dir.join(format!(
        ".{CURRENT_PAIRING_USER_FILE}.{}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, format!("{user_id}\n"))
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, &path).map_err(|error| format!("persist {}: {error}", path.display()))
}

fn save_hex_key_map(path: &Path, values: &HashMap<Vec<u8>, Vec<u8>>) -> Result<(), String> {
    let keyed: HashMap<String, Vec<u8>> = values
        .iter()
        .map(|(key, value)| (hex::encode(key), value.clone()))
        .collect();
    save_string_map(path, &keyed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_storage_paths(root: &Path, session: &str) -> CliStoragePaths {
        CliStoragePaths::new(root.to_path_buf(), root.join("storage").join(session))
    }

    #[test]
    fn connected_user_id_prefers_the_full_username() {
        let info = SessionUiInfo {
            lite_username: Some("alice.dot".to_string()),
            full_username: Some("Alice".to_string()),
            ..SessionUiInfo::default()
        };
        assert_eq!(connected_user_id(&info), Some("Alice"));

        let info = SessionUiInfo {
            lite_username: Some("alice.dot".to_string()),
            ..SessionUiInfo::default()
        };
        assert_eq!(connected_user_id(&info), Some("alice.dot"));
        assert_eq!(connected_user_id(&SessionUiInfo::default()), None);
    }

    #[test]
    fn pairing_storage_switches_with_the_connected_username() {
        let temporary = tempdir().expect("create pairing storage root");
        let network_dir = temporary.path().join("testnet");
        let platform = CliPlatform::new(
            "",
            &[],
            Some(CliStoragePaths::pairing(network_dir.clone())),
            ApprovalPolicy::AutoAccept,
            None,
        );
        let product_key =
            ProductStorageKey::new("product.dot", "theme").expect("product storage key");

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("select alice");
        futures::executor::block_on(platform.write(product_key.encode(), b"dark".to_vec()))
            .expect("write alice product value");

        platform
            .switch_pairing_user_storage("bob.dot")
            .expect("select bob");
        assert_eq!(
            futures::executor::block_on(platform.read(product_key.encode()))
                .expect("read bob product value"),
            None
        );

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("restore alice");
        assert_eq!(
            futures::executor::block_on(platform.read(product_key.encode()))
                .expect("read alice product value"),
            Some(b"dark".to_vec())
        );
        assert_eq!(
            platform.state_dir().as_deref(),
            Some(network_dir.join("alice.dot_pairing_host").as_path())
        );
        assert_eq!(
            read_current_pairing_user(&network_dir.join("pairing-host")).as_deref(),
            Some("alice.dot")
        );
    }

    #[test]
    fn legacy_pairing_storage_moves_to_the_first_resolved_user() {
        let temporary = tempdir().expect("create pairing storage root");
        let network_dir = temporary.path().join("testnet");
        let legacy_product_dir = network_dir.join("pairing-host/storage/default");
        let product_key =
            ProductStorageKey::new("product.dot", "theme").expect("product storage key");
        save_product_storage(
            &legacy_product_dir,
            "product.dot",
            &HashMap::from([("theme".to_string(), b"dark".to_vec())]),
        )
        .expect("write legacy pairing product storage");
        let platform = CliPlatform::new(
            "",
            &[],
            Some(CliStoragePaths::pairing(network_dir.clone())),
            ApprovalPolicy::AutoAccept,
            None,
        );

        platform
            .switch_pairing_user_storage("alice.dot")
            .expect("resolve legacy storage owner");

        assert_eq!(
            futures::executor::block_on(platform.read(product_key.encode()))
                .expect("read migrated product value"),
            Some(b"dark".to_vec())
        );
        assert!(network_dir.join("alice.dot_pairing_host/storage").is_dir());
    }

    #[test]
    fn approval_summaries_are_concise_and_do_not_dump_payloads() {
        let review =
            UserConfirmationReview::PreimageSubmit(truapi_platform::PreimageSubmitReview {
                size: 4_096,
            });

        let (action, detail) = approval_summary(&review);

        assert_eq!(action, "submit preimage");
        assert_eq!(
            detail,
            "A product requested submission of a 4096-byte preimage."
        );
        assert!(!detail.contains("["));
    }

    #[test]
    fn statement_proof_approval_names_product_without_dumping_payload() {
        let review = UserConfirmationReview::StatementStoreProductSign(
            truapi_platform::StatementStoreProductSignReview {
                account: api::ProductAccountId {
                    dot_ns_identifier: "myapp.dot".to_string(),
                    derivation_index: 0,
                },
                payload: vec![0x42; 128],
            },
        );

        let (action, detail) = approval_summary(&review);

        assert_eq!(action, "sign statement proof");
        assert_eq!(
            detail,
            "Product myapp.dot requested a Statement Store proof signature over a 128-byte payload."
        );
        assert!(!detail.contains("[66"));
    }

    #[test]
    fn cli_notifications_return_stable_ids_and_cancel_idempotently() {
        let platform = CliPlatform::new("", &[], None, ApprovalPolicy::AutoAccept, None);
        let first = futures::executor::block_on(platform.push_notification(
            api::HostPushNotificationRequest {
                text: "Hello".to_string(),
                deeplink: None,
                scheduled_at: None,
            },
        ))
        .expect("immediate notification");
        let second = futures::executor::block_on(platform.push_notification(
            api::HostPushNotificationRequest {
                text: "Again".to_string(),
                deeplink: Some("polkadot://example".to_string()),
                scheduled_at: None,
            },
        ))
        .expect("second notification");

        assert_eq!(first.id, 1);
        assert_eq!(second.id, 2);
        futures::executor::block_on(platform.cancel_notification(first.id))
            .expect("already-fired cancellation is idempotent");
        futures::executor::block_on(platform.cancel_notification(999))
            .expect("unknown cancellation is idempotent");
    }

    #[test]
    fn product_storage_uses_safe_per_product_files() {
        let temporary = tempdir().expect("create product storage root");
        let first = ProductStorageKey::new("first.dot", "theme").expect("first product key");
        let localhost =
            ProductStorageKey::new("localhost:3000", "theme").expect("localhost product key");
        let platform = CliPlatform::new(
            "",
            &[],
            Some(test_storage_paths(temporary.path(), "test")),
            ApprovalPolicy::AutoAccept,
            None,
        );

        futures::executor::block_on(async {
            platform
                .write(first.encode(), b"dark".to_vec())
                .await
                .expect("write first product");
            platform
                .write(localhost.encode(), b"light".to_vec())
                .await
                .expect("write localhost product");
        });

        let directory = temporary.path().join("storage").join("test");
        let files = fs::read_dir(&directory)
            .expect("list product storage")
            .map(|entry| entry.expect("product storage entry").path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|path| {
            path.parent() == Some(directory.as_path())
                && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(':'))
        }));

        drop(platform);
        let restored = CliPlatform::new(
            "",
            &[],
            Some(test_storage_paths(temporary.path(), "test")),
            ApprovalPolicy::AutoAccept,
            None,
        );
        let (first_value, localhost_value) = futures::executor::block_on(async {
            (
                restored.read(first.encode()).await.expect("read first"),
                restored
                    .read(localhost.encode())
                    .await
                    .expect("read localhost"),
            )
        });
        assert_eq!(first_value, Some(b"dark".to_vec()));
        assert_eq!(localhost_value, Some(b"light".to_vec()));
    }

    #[test]
    fn product_storage_is_isolated_by_session_then_product() {
        let temporary = tempdir().expect("create session storage root");
        let key = ProductStorageKey::new("same.dot", "value").expect("product key");
        let first = CliPlatform::new(
            "",
            &[],
            Some(test_storage_paths(temporary.path(), "first")),
            ApprovalPolicy::AutoAccept,
            None,
        );
        let second = CliPlatform::new(
            "",
            &[],
            Some(test_storage_paths(temporary.path(), "second")),
            ApprovalPolicy::AutoAccept,
            None,
        );

        futures::executor::block_on(async {
            first
                .write(key.encode(), b"one".to_vec())
                .await
                .expect("write first session");
            second
                .write(key.encode(), b"two".to_vec())
                .await
                .expect("write second session");
        });

        let first_path =
            product_storage_path(&temporary.path().join("storage").join("first"), "same.dot");
        let second_path =
            product_storage_path(&temporary.path().join("storage").join("second"), "same.dot");
        assert!(first_path.is_file());
        assert!(second_path.is_file());
        assert_ne!(
            fs::read_to_string(first_path).expect("read first session file"),
            fs::read_to_string(second_path).expect("read second session file")
        );
    }

    #[test]
    fn legacy_product_storage_migrates_and_keeps_a_backup() {
        let temporary = tempdir().expect("create migration root");
        let first = ProductStorageKey::new("first.dot", "alpha").expect("first product key");
        let second = ProductStorageKey::new("second.dot", "beta").expect("second product key");
        let legacy_path = temporary.path().join("product-storage.json");
        save_string_map(
            &legacy_path,
            &HashMap::from([
                (first.encode(), b"one".to_vec()),
                (second.encode(), b"two".to_vec()),
            ]),
        )
        .expect("write legacy product storage");

        let platform = CliPlatform::new(
            "",
            &[],
            Some(test_storage_paths(temporary.path(), "test")),
            ApprovalPolicy::AutoAccept,
            None,
        );

        assert!(!legacy_path.exists());
        assert!(
            temporary
                .path()
                .join("product-storage.v1.json.migrated")
                .is_file()
        );
        assert_eq!(
            fs::read_dir(temporary.path().join("storage").join("test"))
                .expect("list migrated product files")
                .count(),
            2
        );
        let values = futures::executor::block_on(async {
            (
                platform.read(first.encode()).await.expect("read first"),
                platform.read(second.encode()).await.expect("read second"),
            )
        });
        assert_eq!(values, (Some(b"one".to_vec()), Some(b"two".to_vec())));
    }

    #[test]
    fn corrupt_legacy_product_storage_is_not_marked_as_migrated() {
        let temporary = tempdir().expect("create corrupt migration root");
        let legacy_path = temporary.path().join("product-storage.json");
        fs::write(&legacy_path, "{not-json").expect("write corrupt legacy storage");

        let _platform = CliPlatform::new(
            "",
            &[],
            Some(test_storage_paths(temporary.path(), "test")),
            ApprovalPolicy::AutoAccept,
            None,
        );

        assert!(legacy_path.is_file());
        assert!(
            !temporary
                .path()
                .join("product-storage.v1.json.migrated")
                .exists()
        );
    }
}
