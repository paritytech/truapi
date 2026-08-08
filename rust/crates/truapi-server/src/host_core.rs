//! Stable host-embedding API for the TrUAPI server runtime.
//!
//! `ProductRuntime` is the target-neutral boundary embedders should use.
//! Platform adapters provide:
//! - a [`truapi_platform::Platform`] implementation for host callbacks,
//! - a task [`Spawner`] for runtime-owned async work,
//! - a [`FrameSink`] for outgoing protocol frames.
//!
//! Target-specific shells such as wasm-bindgen, iOS FFI, or desktop IPC should
//! keep their conversion code outside this module.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::future::{AbortHandle, Abortable};
use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
use tracing::instrument;
use truapi::v01;
use truapi_platform::ChatPlatform;
use truapi_platform::{
    CoreAdmin, PairingHostAdmin, PairingHostConfig, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, Platform, ProductContext, SigningHostConfig,
};

use crate::core::TrUApiCore;
use crate::frame::ProtocolMessage;
use crate::runtime::{
    ChatConnection, LocalActivation, PairingHostRole, ProductAuthority, ProductRuntimeHost,
    ResponderExit, RuntimeServices, SigningHostRole, respond_to_pairing,
};
use crate::subscription::{HostInitiatedSubscriptionManager, Spawner};
use crate::transport::Transport;

/// Outgoing frame sink owned by a host adapter.
///
/// Implementations bridge encoded TrUAPI protocol frames to their target
/// transport: JS callbacks, native callbacks, IPC, channels, or another
/// host-specific mechanism.
pub trait FrameSink: Send + Sync {
    /// Emit one SCALE-encoded [`ProtocolMessage`] frame.
    fn emit_frame(&self, frame: Vec<u8>);
}

/// Errors returned by [`ProductRuntime::receive_frame`].
#[derive(Debug, Error)]
pub enum ProductRuntimeError {
    /// Incoming bytes did not decode as a protocol frame.
    #[error("invalid frame: {reason}")]
    InvalidFrame {
        /// Decode failure reason.
        reason: String,
    },
    /// The connection execution kind does not allow the operation.
    #[error("operation denied for this execution")]
    Denied,
    /// The product connection has already closed.
    #[error("product connection is closed")]
    Closed,
    /// The product or native host did not install the requested surface.
    #[error("operation is unsupported")]
    Unsupported,
    /// The bounded pre-subscription action queue is full.
    #[error("connection action buffer is full")]
    BufferFull,
}

fn product_context(product_id: &str) -> Result<ProductContext, v01::GenericError> {
    ProductContext::new(product_id.to_string()).map_err(|err| v01::GenericError {
        reason: err.to_string(),
    })
}

/// A seedless pairing host: the user's keys live in an external wallet reached
/// over the SSO pairing channel.
///
/// Owns the shared services plus pairing-host state. Local-session activation
/// is a signing-host operation and is not present here.
pub struct PairingHostRuntime {
    services: Arc<RuntimeServices>,
    pairing_host: Arc<PairingHostRole>,
}

impl PairingHostRuntime {
    /// Build a long-lived pairing-host runtime around a platform implementation.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.new"))]
    pub fn new<P>(platform: Arc<P>, config: PairingHostConfig, spawner: Spawner) -> Self
    where
        P: Platform + 'static,
    {
        let platform: Arc<dyn Platform> = platform;
        let services = RuntimeServices::new(
            platform,
            config.people_chain_genesis_hash,
            config.bulletin_chain_genesis_hash,
            spawner.clone(),
        );
        let pairing_host = PairingHostRole::new(services.clone(), config);
        pairing_host.clone().start_session_store_sync(spawner);
        Self {
            services,
            pairing_host,
        }
    }

    /// Build a product-facing runtime from this pairing host.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.product_runtime"))]
    pub fn product_runtime(
        &self,
        product: ProductContext,
        sink: Arc<dyn FrameSink>,
    ) -> ProductRuntime {
        ProductRuntime::new(
            self.services.clone(),
            self.pairing_host.clone(),
            product,
            ConnectionAdapters::from_services(&self.services),
            sink,
        )
    }

    /// Build a product-scoped administration handle from this pairing host.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.product_admin"))]
    pub fn product_admin(&self, product: ProductContext) -> HostAdmin {
        HostAdmin::new(
            self.services.clone(),
            self.pairing_host.clone(),
            product,
            ConnectionAdapters::from_services(&self.services),
        )
    }

    /// Disconnect the active account-authority session.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.disconnect_session"))]
    pub async fn disconnect_session(&self) {
        self.pairing_host.disconnect().await;
    }

    /// Log out and discard the old pairing keypair.
    ///
    /// The next product login request generates a fresh pairing identity and
    /// presents a new deeplink suitable for another signing host.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.logout"))]
    pub async fn logout(&self) -> Result<(), v01::GenericError> {
        self.pairing_host
            .logout_and_reset_pairing()
            .await
            .map_err(|reason| v01::GenericError { reason })
    }

    /// Clear one product's capability state while preserving the active
    /// session and unrelated products.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.clear_product_state", %product_id))]
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), v01::GenericError> {
        self.pairing_host
            .clear_product_state(product_id)
            .await
            .map_err(|reason| v01::GenericError { reason })
    }

    /// Clear the canonical paired session and all capability caches/storage
    /// without sending a peer-disconnect notice.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.reset_session_state"))]
    pub async fn reset_session_state(&self) {
        self.pairing_host.reset_session_state().await;
    }

    /// Start or join the pairing-host login flow for one product.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.login", %product_id))]
    pub async fn login(
        &self,
        product_id: &str,
    ) -> Result<v01::HostRequestLoginResponse, v01::GenericError> {
        let product = product_context(product_id)?;
        match self.pairing_host.request_login(&product).await {
            Ok(truapi::versioned::account::HostRequestLoginResponse::V1(response)) => Ok(response),
            Err(error) => Err(v01::GenericError {
                reason: pairing_login_error_reason(error),
            }),
        }
    }

    /// Cancel an in-flight SSO pairing request. A no-op when no pairing is
    /// active.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.cancel_pairing"))]
    pub fn cancel_pairing(&self) {
        self.pairing_host.cancel_login();
    }

    /// Activate a canonical session blob supplied by an external encrypted
    /// session owner without writing the blob to core storage.
    ///
    /// Success means decoding, username resolution, replacement fencing, and
    /// connected-session installation have completed.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.activate_external_session"))]
    pub async fn activate_external_session(&self, blob: &[u8]) -> Result<(), v01::GenericError> {
        self.pairing_host
            .activate_external_session(blob)
            .await
            .map_err(|reason| v01::GenericError { reason })
    }

    /// Await restoration of the persisted auth-session blob.
    ///
    /// Success means decoding, username resolution, stale-read fencing, and
    /// connected-session installation have completed, so product frames may
    /// immediately use the restored authority session.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.activate_stored_session"))]
    pub async fn activate_stored_session(&self) -> Result<(), v01::GenericError> {
        self.pairing_host
            .activate_stored_session()
            .await
            .map_err(|reason| v01::GenericError { reason })
    }

    /// Notify the pairing runtime that the persisted auth-session blob may
    /// have changed and should be re-read.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.notify_session_store_changed"))]
    pub fn notify_session_store_changed(&self) {
        self.pairing_host.notify_session_store_changed();
    }

    /// Read a stored permission authorization status for a product without prompting.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.permission_authorization_status", product_id = %product_id))]
    pub async fn permission_authorization_status(
        &self,
        product_id: &str,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        self.product_admin(product_context(product_id)?)
            .permission_authorization_status(request)
            .await
    }

    /// Read stored permission authorization statuses for a product without prompting.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.permission_authorization_statuses", product_id = %product_id))]
    pub async fn permission_authorization_statuses(
        &self,
        product_id: &str,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        self.product_admin(product_context(product_id)?)
            .permission_authorization_statuses(requests)
            .await
    }

    /// Update a stored permission authorization status for a product.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.set_permission_authorization_status", product_id = %product_id))]
    pub async fn set_permission_authorization_status(
        &self,
        product_id: &str,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        self.product_admin(product_context(product_id)?)
            .set_permission_authorization_status(request, status)
            .await
    }
}

fn pairing_login_error_reason(
    error: truapi::CallError<truapi::versioned::account::HostRequestLoginError>,
) -> String {
    match error {
        truapi::CallError::Domain(truapi::versioned::account::HostRequestLoginError::V1(
            v01::HostRequestLoginError::Unknown { reason },
        ))
        | truapi::CallError::HostFailure { reason }
        | truapi::CallError::MalformedFrame { reason } => reason,
        truapi::CallError::Denied => "login denied".to_string(),
        truapi::CallError::Unsupported => "login unsupported".to_string(),
    }
}

impl PairingHostAdmin for PairingHostRuntime {
    fn cancel_pairing(&self) {
        PairingHostRuntime::cancel_pairing(self);
    }

    fn notify_session_store_changed(&self) {
        PairingHostRuntime::notify_session_store_changed(self);
    }
}

/// A wallet-local signing host: the user's keys are held on this device.
///
/// Owns the shared services plus signing-host state. There is no pairing flow,
/// so pairing cancellation is not present here.
///
/// Raw-bytes and extrinsic-payload signing, v4 transaction construction, and
/// product entropy are implemented; native signing hosts can also serve
/// ring-VRF aliases and on-chain resource allocation.
pub struct SigningHostRuntime {
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHostRole>,
}

impl SigningHostRuntime {
    /// Build a long-lived signing-host runtime around a platform implementation.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.new"))]
    pub fn new<P>(platform: Arc<P>, config: SigningHostConfig, spawner: Spawner) -> Self
    where
        P: Platform + 'static,
    {
        let platform: Arc<dyn Platform> = platform;
        let services = RuntimeServices::new(
            platform,
            config.people_chain_genesis_hash,
            config.bulletin_chain_genesis_hash,
            spawner,
        );
        let signing_host = SigningHostRole::new(services.clone());
        Self {
            services,
            signing_host,
        }
    }

    /// Build a product-facing runtime from this signing host.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.product_runtime"))]
    pub fn product_runtime(
        &self,
        product: ProductContext,
        sink: Arc<dyn FrameSink>,
    ) -> ProductRuntime {
        ProductRuntime::new(
            self.services.clone(),
            self.signing_host.clone(),
            product,
            ConnectionAdapters::from_services(&self.services),
            sink,
        )
    }

    /// Build one product connection with adapters scoped to one native
    /// executable while sharing this runtime's authentication and services.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn product_runtime_with(
        &self,
        product: ProductContext,
        adapters: ConnectionAdapters,
        sink: Arc<dyn FrameSink>,
    ) -> ProductRuntime {
        ProductRuntime::new(
            self.services.clone(),
            self.signing_host.clone(),
            product,
            adapters,
            sink,
        )
    }

    /// Build a product-scoped administration handle from this signing host.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.product_admin"))]
    pub fn product_admin(&self, product: ProductContext) -> HostAdmin {
        HostAdmin::new(
            self.services.clone(),
            self.signing_host.clone(),
            product,
            ConnectionAdapters::from_services(&self.services),
        )
    }

    /// Build a product administration handle with adapters scoped to one
    /// native executable connection.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn product_admin_with(
        &self,
        product: ProductContext,
        adapters: ConnectionAdapters,
    ) -> HostAdmin {
        HostAdmin::new(
            self.services.clone(),
            self.signing_host.clone(),
            product,
            adapters,
        )
    }

    /// Return whether this host currently has an authenticated signing session.
    pub fn has_active_session(&self) -> bool {
        self.signing_host.session_state().current().is_some()
    }

    /// Disconnect the active account-authority session.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.disconnect_session"))]
    pub async fn disconnect_session(&self) {
        self.signing_host.disconnect().await;
    }

    /// Revoke one product's grants from the current local activation while
    /// preserving unrelated products.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.clear_product_state", %product_id))]
    pub async fn clear_product_state(&self, product_id: &str) -> Result<(), v01::GenericError> {
        self.signing_host
            .clear_product_state(product_id)
            .map_err(|error| v01::GenericError {
                reason: error.to_string(),
            })
    }

    /// Activate a wallet-local session from host-held secret material (raw
    /// BIP-39 entropy).
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.activate_local_session"))]
    pub async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), v01::GenericError> {
        self.signing_host
            .activate_local_session(secret)
            .await
            .map_err(|err| v01::GenericError {
                reason: err.to_string(),
            })
    }

    /// Activate a wallet-local session from host-held secret material and
    /// attach known identity metadata.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.activate_local_session_with_identity"))]
    pub async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), v01::GenericError> {
        self.signing_host
            .activate_local_session_with_identity(secret, lite_username)
            .await
            .map_err(|err| v01::GenericError {
                reason: err.to_string(),
            })
    }

    /// Answer a pairing host's handshake deeplink and serve the resulting SSO
    /// session until it ends.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.respond_to_pairing"))]
    pub async fn respond_to_pairing(
        &self,
        deeplink: &str,
    ) -> Result<ResponderExit, v01::GenericError> {
        respond_to_pairing(self.services.clone(), self.signing_host.clone(), deeplink)
            .await
            .map_err(|reason| v01::GenericError { reason })
    }
}

/// Adapters scoped to one product connection: the platform serving its
/// syscalls, the optional native Chat adapter, and the connection's Chat
/// stream state. Non-native connections use [`Self::from_services`].
pub(crate) struct ConnectionAdapters {
    pub(crate) platform: Arc<dyn Platform>,
    pub(crate) chat_platform: Option<Arc<dyn ChatPlatform>>,
    pub(crate) chat: Arc<ChatConnection>,
}

impl ConnectionAdapters {
    /// Default adapters for a connection without native scoping.
    pub(crate) fn from_services(services: &RuntimeServices) -> Self {
        Self {
            platform: services.platform.clone(),
            chat_platform: None,
            chat: Arc::new(ChatConnection::new()),
        }
    }
}

/// Product-scoped administration handle for host UI.
///
/// Host UI should use this when it needs to inspect or update core-owned state
/// without owning a product frame endpoint.
pub struct HostAdmin {
    authority: Arc<dyn ProductAuthority>,
    product_runtime: Arc<ProductRuntimeHost>,
}

impl HostAdmin {
    /// Build an admin handle from a long-lived host runtime and the adapters
    /// scoped to one product connection.
    #[instrument(skip_all, fields(runtime.method = "host_admin.new"))]
    pub(crate) fn new(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        product: ProductContext,
        adapters: ConnectionAdapters,
    ) -> Self {
        let product_runtime = Arc::new(ProductRuntimeHost::from_services(
            services,
            adapters,
            authority.clone(),
            product,
        ));
        Self {
            authority,
            product_runtime,
        }
    }

    /// Core-owned logout/disconnect.
    #[instrument(skip_all, fields(runtime.method = "host_admin.disconnect_session"))]
    pub async fn disconnect_session(&self) {
        self.authority.disconnect().await;
    }

    /// Read a stored permission authorization status without prompting.
    #[instrument(skip_all, fields(runtime.method = "host_admin.permission_authorization_status"))]
    pub async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        self.product_runtime
            .permission_authorization_status(request)
            .await
    }

    /// Read stored permission authorization statuses without prompting.
    #[instrument(skip_all, fields(runtime.method = "host_admin.permission_authorization_statuses"))]
    pub async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        self.product_runtime
            .permission_authorization_statuses(requests)
            .await
    }

    /// Update a stored permission authorization status.
    #[instrument(skip_all, fields(runtime.method = "host_admin.set_permission_authorization_status"))]
    pub async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        self.product_runtime
            .set_permission_authorization_status(request, status)
            .await
    }
}

#[truapi_platform::async_trait]
impl CoreAdmin for HostAdmin {
    async fn disconnect_session(&self) -> Result<(), v01::GenericError> {
        HostAdmin::disconnect_session(self).await;
        Ok(())
    }

    async fn get_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        self.permission_authorization_status(request).await
    }

    async fn get_permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        self.permission_authorization_statuses(requests).await
    }

    async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        HostAdmin::set_permission_authorization_status(self, request, status).await
    }
}

/// Target-neutral host runtime wrapper.
///
/// `ProductRuntime` is product-scoped. It owns the dispatcher core for one product
/// connection and handles byte-frame ingress, response/subscription egress, and
/// in-flight dispatch cancellation on dispose.
pub struct ProductRuntime {
    core: TrUApiCore,
    admin: HostAdmin,
    transport: Arc<SinkTransport>,
    host_subscriptions: Arc<HostInitiatedSubscriptionManager>,
    disposed: Arc<AtomicBool>,
    in_flight: Mutex<HashMap<u64, AbortHandle>>,
    next_dispatch_id: AtomicU64,
}

/// Host-facing control handle for pushing native events into one concrete
/// product connection.
#[derive(Clone)]
pub struct ProductRuntimeControl {
    runtime: Arc<ProductRuntimeHost>,
    transport: Arc<SinkTransport>,
    host_subscriptions: Arc<HostInitiatedSubscriptionManager>,
    disposed: Arc<AtomicBool>,
}

impl ProductRuntimeControl {
    fn runtime(&self) -> Result<&ProductRuntimeHost, ProductRuntimeError> {
        if self.disposed.load(Ordering::Acquire) {
            return Err(ProductRuntimeError::Closed);
        }
        Ok(&self.runtime)
    }

    /// Request custom-message UI from this connection's product renderer.
    pub fn render_custom_message(
        &self,
        message_id: String,
        message_type: String,
        payload: Vec<u8>,
    ) -> Result<truapi::Subscription<v01::CustomRendererNode>, ProductRuntimeError> {
        self.runtime()?.native_chat_platform()?;
        let request = truapi::versioned::chat::ProductChatCustomMessageRenderRequest::V1(
            v01::ProductChatCustomMessageRenderRequest {
                message_id,
                message_type,
                payload,
            },
        );
        let transport: Arc<dyn Transport> = self.transport.clone();
        let stream = crate::generated::dispatcher::chat_custom_message_render(
            &self.host_subscriptions,
            transport,
            request,
        )
        .map(|item| match item {
            truapi::versioned::chat::ProductChatCustomMessageRenderItem::V1(node) => node,
        });
        Ok(truapi::Subscription::new(Box::pin(stream)))
    }
}

impl ProductRuntime {
    /// Build a product-facing host core around a platform implementation and
    /// outgoing frame sink.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.from_platform_with_config"))]
    pub fn from_platform_with_config<P>(
        platform: Arc<P>,
        host_config: PairingHostConfig,
        product: ProductContext,
        spawner: Spawner,
        sink: Arc<dyn FrameSink>,
    ) -> Self
    where
        P: Platform + 'static,
    {
        let pairing = PairingHostRuntime::new(platform, host_config, spawner);
        pairing.product_runtime(product, sink)
    }

    /// Build a product-facing runtime from shared services and an authority.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.new"))]
    pub(crate) fn new(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        product: ProductContext,
        adapters: ConnectionAdapters,
        sink: Arc<dyn FrameSink>,
    ) -> Self {
        let admin = HostAdmin::new(services.clone(), authority.clone(), product, adapters);
        let disposed = Arc::new(AtomicBool::new(false));
        let transport = Arc::new(SinkTransport {
            sink,
            disposed: disposed.clone(),
        });
        let host_subscriptions = Arc::new(HostInitiatedSubscriptionManager::new());
        Self {
            core: TrUApiCore::from_product_runtime(
                admin.product_runtime.clone(),
                services.spawner.clone(),
                authority.session_state(),
            ),
            admin,
            transport,
            host_subscriptions,
            disposed,
            in_flight: Mutex::new(HashMap::new()),
            next_dispatch_id: AtomicU64::new(0),
        }
    }

    /// Push one SCALE-encoded protocol frame into the dispatcher.
    ///
    /// Calls after [`Self::dispose`] are ignored and return `Ok(())` without
    /// decoding. If dispose happens while a dispatch is in flight, the dispatch
    /// is aborted and this method still returns `Ok(())`.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.receive_frame"))]
    pub async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), ProductRuntimeError> {
        if self.disposed.load(Ordering::Acquire) {
            return Ok(());
        }

        let message = ProtocolMessage::decode(&mut frame.as_slice()).map_err(|err| {
            ProductRuntimeError::InvalidFrame {
                reason: err.to_string(),
            }
        })?;
        let Some(message) = self.host_subscriptions.handle_message(message) else {
            return Ok(());
        };
        let dispatch_id = self.next_dispatch_id.fetch_add(1, Ordering::Relaxed);
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        self.in_flight
            .lock()
            .expect("host core in-flight dispatch mutex poisoned")
            .insert(dispatch_id, abort_handle);

        let transport: Arc<dyn Transport> = self.transport.clone();
        let _ = Abortable::new(self.core.dispatch(message, transport), abort_registration).await;

        self.in_flight
            .lock()
            .expect("host core in-flight dispatch mutex poisoned")
            .remove(&dispatch_id);
        if self.disposed.load(Ordering::Acquire) {
            self.core.cancel_subscriptions();
        }
        Ok(())
    }

    /// Return a cloneable native control handle bound to this connection.
    pub fn control(&self) -> ProductRuntimeControl {
        ProductRuntimeControl {
            runtime: self.admin.product_runtime.clone(),
            transport: self.transport.clone(),
            host_subscriptions: self.host_subscriptions.clone(),
            disposed: self.disposed.clone(),
        }
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.disconnect_session"))]
    pub async fn disconnect_session(&self) {
        self.admin.disconnect_session().await;
    }

    /// Read a stored permission authorization status without prompting.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.permission_authorization_status"))]
    pub async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        self.admin.permission_authorization_status(request).await
    }

    /// Read stored permission authorization statuses without prompting.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.permission_authorization_statuses"))]
    pub async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        self.admin.permission_authorization_statuses(requests).await
    }

    /// Update a stored permission authorization status. `NotDetermined`
    /// clears the stored value so the next product request prompts again.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.set_permission_authorization_status"))]
    pub async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        self.admin
            .set_permission_authorization_status(request, status)
            .await
    }

    /// Dispose this host core. Idempotent.
    ///
    /// Disposal suppresses future outgoing frames, aborts in-flight dispatch
    /// futures, and cancels active subscriptions.
    #[instrument(skip_all, fields(runtime.method = "product_runtime.dispose"))]
    pub fn dispose(&self) {
        if self.disposed.swap(true, Ordering::AcqRel) {
            return;
        }
        for (_, handle) in self
            .in_flight
            .lock()
            .expect("host core in-flight dispatch mutex poisoned")
            .drain()
        {
            handle.abort();
        }
        self.admin.product_runtime.detach_chat();
        self.host_subscriptions.close();
        self.core.cancel_subscriptions();
    }
}

struct SinkTransport {
    sink: Arc<dyn FrameSink>,
    disposed: Arc<AtomicBool>,
}

impl Transport for SinkTransport {
    fn send(&self, message: ProtocolMessage) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }
        self.sink.emit_frame(message.encode());
    }

    fn on_message(
        &self,
        _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
    ) -> Box<dyn FnOnce()> {
        Box::new(|| {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Payload, ProtocolMessage, subscription_ids};
    use crate::test_support::{StubPlatform, runtime_config, test_spawner};
    use parity_scale_codec::Encode;
    use std::sync::atomic::Ordering;

    #[derive(Default)]
    struct RecordingSink {
        frames: Mutex<Vec<Vec<u8>>>,
    }

    impl FrameSink for RecordingSink {
        fn emit_frame(&self, frame: Vec<u8>) {
            self.frames
                .lock()
                .expect("recording sink mutex poisoned")
                .push(frame);
        }
    }

    fn assert_send<T: Send>(_: T) {}

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn product_runtime_and_dispatch_future_are_send() {
        assert_send_sync::<ProductRuntime>();
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
            test_spawner(),
            Arc::new(RecordingSink::default()),
        );

        assert_send(runtime.receive_frame(Vec::new()));
    }

    #[test]
    fn spa_connection_rejects_native_custom_rendering() {
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
            test_spawner(),
            Arc::new(RecordingSink::default()),
        );

        assert!(matches!(
            runtime
                .control()
                .render_custom_message("message".into(), "vote".into(), vec![]),
            Err(ProductRuntimeError::Denied)
        ));
    }

    #[test]
    fn generated_filter_denies_chat_request_on_spa_connection() {
        let sink = Arc::new(RecordingSink::default());
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
            test_spawner(),
            sink.clone(),
        );
        let ids = crate::frame::request_ids("chat_create_room").expect("known Chat request");
        let request = truapi::versioned::chat::HostChatCreateRoomRequest::V1(
            v01::HostChatCreateRoomRequest {
                room_id: "room".into(),
                name: "Room".into(),
                icon: String::new(),
            },
        );
        let frame = ProtocolMessage {
            request_id: "chat:1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        };

        futures::executor::block_on(runtime.receive_frame(frame.encode())).unwrap();

        let frames = sink.frames.lock().unwrap();
        assert_eq!(frames.len(), 1);
        let response = ProtocolMessage::decode(&mut frames[0].as_slice()).unwrap();
        assert_eq!(response.payload.id, ids.response_id);
        let expected = crate::frame::encode_versioned_err_payload(
            truapi::CallError::<truapi::versioned::chat::HostChatCreateRoomError>::Denied,
            1,
        );
        assert_eq!(response.payload.value, expected);
    }

    #[test]
    fn generated_filter_denies_chat_subscription_on_spa_connection() {
        let sink = Arc::new(RecordingSink::default());
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
            test_spawner(),
            sink.clone(),
        );
        let ids = subscription_ids("chat_action_subscribe").expect("known Chat subscription");
        let frame = ProtocolMessage {
            request_id: "chat:actions".into(),
            payload: Payload {
                id: ids.start_id,
                value: Vec::new(),
            },
        };

        futures::executor::block_on(runtime.receive_frame(frame.encode())).unwrap();

        let frames = sink.frames.lock().unwrap();
        assert_eq!(frames.len(), 1);
        let response = ProtocolMessage::decode(&mut frames[0].as_slice()).unwrap();
        assert_eq!(response.request_id, "chat:actions");
        assert_eq!(response.payload.id, ids.interrupt_id);
        assert!(response.payload.value.is_empty());
    }

    #[test]
    fn dispose_cancels_active_subscriptions() {
        let theme_stream_dropped = Arc::new(AtomicBool::new(false));
        let platform = Arc::new(StubPlatform {
            theme_stream_pending: true,
            theme_stream_dropped: theme_stream_dropped.clone(),
            ..Default::default()
        });
        let sink = Arc::new(RecordingSink::default());
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            platform,
            host_config,
            product,
            test_spawner(),
            sink,
        );

        let ids = subscription_ids("theme_subscribe").expect("known subscription");
        let frame = ProtocolMessage {
            request_id: "theme:1".to_string(),
            payload: Payload {
                id: ids.start_id,
                value: Vec::new(),
            },
        };
        futures::executor::block_on(runtime.receive_frame(frame.encode())).unwrap();

        runtime.dispose();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !theme_stream_dropped.load(Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < deadline,
                "dispose did not drop the active theme subscription stream"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
