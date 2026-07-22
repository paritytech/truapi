//! `Platform` implementation for the headless hosts.
//!
//! In-memory product and core storage, a WebSocket chain provider pointed at
//! the real People-chain statement store, and a [`UserConfirmation`] that
//! either auto-accepts or prompts on the CLI (the web/iOS "sign?" modal).
//! Auth-state transitions are published on a channel so the CLI can print the
//! pairing deeplink and observe connection status.

use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use truapi::latest as api;
use truapi_platform::{
    AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features, JsonRpcConnection, Navigation,
    Notifications, Permissions, PreimageHost, ProductStorage, SessionUiInfo, ThemeHost,
    UserConfirmation, UserConfirmationReview,
};

use crate::chain::WsChainProvider;
use crate::terminal_ui::{SystemEvent, UiHandle};

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
        storage_dir: Option<PathBuf>,
        approval: ApprovalPolicy,
        ui: Option<UiHandle>,
    ) -> Arc<Self> {
        let (product_storage_path, core_storage_path) = storage_dir
            .as_ref()
            .map(|dir| {
                if let Err(err) = fs::create_dir_all(dir) {
                    tracing::warn!(path = %dir.display(), %err, "could not create CLI storage dir");
                }
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
    ) -> Result<(), api::HostLocalStorageReadError> {
        {
            self.product_storage
                .lock()
                .expect("product storage mutex poisoned")
                .insert(key, value);
        }
        self.persist_product_storage()
            .map_err(|reason| api::HostLocalStorageReadError::Unknown { reason })
    }

    async fn clear(&self, key: String) -> Result<(), api::HostLocalStorageReadError> {
        {
            self.product_storage
                .lock()
                .expect("product storage mutex poisoned")
                .remove(&key);
        }
        self.persist_product_storage()
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
        tracing::info!(%url, "navigate_to");
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
                connected_label(info),
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

fn connected_label(info: &SessionUiInfo) -> String {
    connected_user_id(info).map_or_else(
        || "connected".to_string(),
        |user_id| format!("connected · {user_id}"),
    )
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

fn load_string_map(path: &Path) -> HashMap<String, Vec<u8>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not read CLI storage");
            return HashMap::new();
        }
    };
    match serde_json::from_str::<JsonMap>(&text) {
        Ok(json) => json
            .values
            .into_iter()
            .filter_map(|(key, value)| hex::decode(value).ok().map(|bytes| (key, bytes)))
            .collect(),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not decode CLI storage");
            HashMap::new()
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_label_includes_the_primary_user_id() {
        let info = SessionUiInfo {
            lite_username: Some("alice.dot".to_string()),
            full_username: Some("Alice".to_string()),
            ..SessionUiInfo::default()
        };
        assert_eq!(connected_label(&info), "connected · Alice");

        let info = SessionUiInfo {
            lite_username: Some("alice.dot".to_string()),
            ..SessionUiInfo::default()
        };
        assert_eq!(connected_label(&info), "connected · alice.dot");
        assert_eq!(connected_label(&SessionUiInfo::default()), "connected");
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
}
