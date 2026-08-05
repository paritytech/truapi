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

use futures::future::{AbortHandle, Abortable};
use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
use tracing::instrument;
use truapi::v01;
use truapi_platform::{
    CoreAdmin, PairingHostAdmin, PairingHostConfig, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, Platform, ProductContext, SigningHostConfig,
};

use crate::core::TrUApiCore;
use crate::frame::ProtocolMessage;
use crate::runtime::{
    LocalActivation, PairingHostRole, ProductAuthority, ProductRuntimeHost, ResponderExit,
    RuntimeServices, SigningHostRole, respond_to_pairing,
};
use crate::subscription::Spawner;
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

/// Dev-only sink that observes host debug events at the core's two frame choke
/// points. A host that does not enable the debugger leaves it unset and the tap
/// is inert. Fire-and-forget by construction: [`DebugSink::emit`] must not block
/// the frame path and must not fail the operation that produced the event, so a
/// slow, absent, or crashed debugger only loses the trace, never a session.
pub trait DebugSink: Send + Sync {
    /// Hand one event to the sink.
    ///
    /// Must not block, and must not panic: `emit` is called from inside the
    /// inbound and outbound frame paths, so a panic here would unwind into a
    /// live dispatch. Serialize and enqueue only; never do fallible work that
    /// can `unwrap`/panic on the caller's thread.
    fn emit(&self, event: DebugEvent);
}

/// Identifies which product channel on a host a debug event belongs to, so one
/// debugger app can demultiplex several channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelId(pub String);

/// Direction of a tapped frame relative to the host core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDirection {
    /// Product to core (inbound to the host).
    In,
    /// Core to product (outbound from the host).
    Out,
}

impl FrameDirection {
    /// The wire direction string, from the **product's** vantage - the vantage
    /// the debugger app and the design doc use: `"out"` = the frame left the
    /// product, `"in"` = it arrived at the product. This is the inverse of the
    /// enum's host-vantage variants (`In` = product to core, i.e. it *left* the
    /// product), so every sink serializes the same product-vantage string
    /// instead of re-deriving (and risking inverting) it.
    pub fn wire_str(self) -> &'static str {
        match self {
            FrameDirection::In => "out",
            FrameDirection::Out => "in",
        }
    }
}

/// Hand one event to a [`DebugSink`] without letting a misbehaving out-of-repo
/// implementation take down a live dispatch.
///
/// The trait contract forbids `emit` from panicking, but the trait is `pub`, so
/// this guards the two in-path call sites: a panic is caught, logged, and
/// swallowed. `DebugEvent` is `UnwindSafe` (a `ChannelId`/`Vec<u8>`), so the
/// caught closure carries no broken invariant across the boundary.
fn emit_debug(sink: &dyn DebugSink, event: DebugEvent) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        sink.emit(event);
    }));
    if result.is_err() {
        tracing::error!("truapi debug sink panicked in emit; frame dropped, session unaffected");
    }
}

/// One observable host debug event. Frame bytes are the untouched
/// `ProtocolMessage`; the debugger decodes them, so the core never does. The
/// enum leaves room for host-internal events (e.g. SSO) that have no wire frame,
/// so it is `#[non_exhaustive]`: adding a variant is not a breaking change.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum DebugEvent {
    /// A SCALE wire frame crossing a product channel.
    Frame {
        /// Which product channel on this host.
        channel_id: ChannelId,
        /// Product to core, or core to product.
        dir: FrameDirection,
        /// Untouched encoded `ProtocolMessage` bytes.
        bytes: Vec<u8>,
    },
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
            platform.clone(),
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
            sink,
        )
    }

    /// Build a product-scoped administration handle from this pairing host.
    #[instrument(skip_all, fields(runtime.method = "pairing_host_runtime.product_admin"))]
    pub fn product_admin(&self, product: ProductContext) -> HostAdmin {
        HostAdmin::new(self.services.clone(), self.pairing_host.clone(), product)
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
            platform.clone(),
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
            sink,
        )
    }

    /// Build a product-scoped administration handle from this signing host.
    #[instrument(skip_all, fields(runtime.method = "signing_host_runtime.product_admin"))]
    pub fn product_admin(&self, product: ProductContext) -> HostAdmin {
        HostAdmin::new(self.services.clone(), self.signing_host.clone(), product)
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

/// Product-scoped administration handle for host UI.
///
/// Host UI should use this when it needs to inspect or update core-owned state
/// without owning a product frame endpoint.
pub struct HostAdmin {
    authority: Arc<dyn ProductAuthority>,
    product_runtime: Arc<ProductRuntimeHost>,
}

impl HostAdmin {
    /// Build an admin handle from a long-lived host runtime.
    #[instrument(skip_all, fields(runtime.method = "host_admin.new"))]
    pub(crate) fn new(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        product: ProductContext,
    ) -> Self {
        let product_runtime = Arc::new(ProductRuntimeHost::from_services(
            services,
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
    disposed: Arc<AtomicBool>,
    in_flight: Mutex<HashMap<u64, AbortHandle>>,
    next_dispatch_id: AtomicU64,
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
        sink: Arc<dyn FrameSink>,
    ) -> Self {
        let disposed = Arc::new(AtomicBool::new(false));
        let transport = Arc::new(SinkTransport {
            sink,
            disposed: disposed.clone(),
            has_debug: AtomicBool::new(false),
            debug: Mutex::new(None),
        });
        let admin = HostAdmin::new(services.clone(), authority.clone(), product);
        Self {
            core: TrUApiCore::from_product_runtime(
                admin.product_runtime.clone(),
                services.spawner.clone(),
                authority.session_state(),
            ),
            admin,
            transport,
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

        // Tap inbound before decode, so a corrupt frame is still observed.
        if let Some((channel_id, debug)) = self.transport.debug() {
            emit_debug(
                debug.as_ref(),
                DebugEvent::Frame {
                    channel_id,
                    dir: FrameDirection::In,
                    bytes: frame.clone(),
                },
            );
        }

        let message = ProtocolMessage::decode(&mut frame.as_slice()).map_err(|err| {
            ProductRuntimeError::InvalidFrame {
                reason: err.to_string(),
            }
        })?;
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

    /// Install a dev-only [`DebugSink`] that observes every product frame in
    /// both directions for `channel_id`. Absent by default and inert in
    /// production; fire-and-forget, so it can never stall or fail a dispatch.
    pub fn set_debug_sink(&self, channel_id: ChannelId, sink: Arc<dyn DebugSink>) {
        self.transport.set_debug_sink(channel_id, sink);
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
        self.core.cancel_subscriptions();
    }
}

struct SinkTransport {
    sink: Arc<dyn FrameSink>,
    disposed: Arc<AtomicBool>,
    /// Fast-path flag: `false` (the production default) lets the per-frame
    /// `debug()` return without touching the mutex. Set once when a sink is
    /// installed; a reader that races the install just misses one frame.
    has_debug: AtomicBool,
    debug: Mutex<Option<(ChannelId, Arc<dyn DebugSink>)>>,
}

impl SinkTransport {
    /// The installed debug sink and its channel, if any. Lock-free `None` on the
    /// production path (no sink installed); only locks once one is.
    fn debug(&self) -> Option<(ChannelId, Arc<dyn DebugSink>)> {
        if !self.has_debug.load(Ordering::Relaxed) {
            return None;
        }
        self.debug
            .lock()
            .expect("host core debug sink mutex poisoned")
            .clone()
    }

    fn set_debug_sink(&self, channel_id: ChannelId, sink: Arc<dyn DebugSink>) {
        *self
            .debug
            .lock()
            .expect("host core debug sink mutex poisoned") = Some((channel_id, sink));
        self.has_debug.store(true, Ordering::Relaxed);
    }
}

impl Transport for SinkTransport {
    fn send(&self, message: ProtocolMessage) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }
        let encoded = message.encode();
        // Forward to the product first, then tap: the debugger is in the path
        // but never in the critical path.
        match self.debug() {
            Some((channel_id, debug)) => {
                self.sink.emit_frame(encoded.clone());
                emit_debug(
                    debug.as_ref(),
                    DebugEvent::Frame {
                        channel_id,
                        dir: FrameDirection::Out,
                        bytes: encoded,
                    },
                );
            }
            None => self.sink.emit_frame(encoded),
        }
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

    #[derive(Default)]
    struct RecordingDebugSink {
        events: Mutex<Vec<(ChannelId, FrameDirection, Vec<u8>)>>,
    }

    impl DebugSink for RecordingDebugSink {
        fn emit(&self, event: DebugEvent) {
            match event {
                DebugEvent::Frame {
                    channel_id,
                    dir,
                    bytes,
                } => self
                    .events
                    .lock()
                    .expect("debug events mutex poisoned")
                    .push((channel_id, dir, bytes)),
            }
        }
    }

    #[test]
    fn debug_sink_taps_frames_in_both_directions() {
        let platform = Arc::new(StubPlatform::default());
        let sink = Arc::new(RecordingSink::default());
        let debug = Arc::new(RecordingDebugSink::default());
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            platform,
            host_config,
            product,
            test_spawner(),
            sink.clone(),
        );
        runtime.set_debug_sink(ChannelId("myapp.dot".to_string()), debug.clone());

        let ids = subscription_ids("theme_subscribe").expect("known subscription");
        let frame = ProtocolMessage {
            request_id: "theme:1".to_string(),
            payload: Payload {
                id: ids.start_id,
                value: Vec::new(),
            },
        };
        let raw = frame.encode();
        futures::executor::block_on(runtime.receive_frame(raw.clone())).unwrap();

        // The subscription's first item is emitted asynchronously; wait for it,
        // then let the tap (which runs right after delivery in `send`) settle.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while sink
            .frames
            .lock()
            .expect("recording sink mutex poisoned")
            .is_empty()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Snapshot into owned vecs (never hold a lock across an assertion).
        let (inbound, outbound, channels): (Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<ChannelId>) = {
            let events = debug.events.lock().expect("debug events mutex poisoned");
            (
                events
                    .iter()
                    .filter(|(_, dir, _)| *dir == FrameDirection::In)
                    .map(|(_, _, bytes)| bytes.clone())
                    .collect(),
                events
                    .iter()
                    .filter(|(_, dir, _)| *dir == FrameDirection::Out)
                    .map(|(_, _, bytes)| bytes.clone())
                    .collect(),
                events.iter().map(|(cid, _, _)| cid.clone()).collect(),
            )
        };
        let delivered = sink
            .frames
            .lock()
            .expect("recording sink mutex poisoned")
            .clone();

        // Every event carries the installed channel id.
        assert!(
            channels
                .iter()
                .all(|c| *c == ChannelId("myapp.dot".to_string())),
            "every event carries its channel id"
        );
        // Inbound tapped once, untouched, before decode.
        assert_eq!(
            inbound,
            vec![raw],
            "inbound frame tapped exactly once, untouched"
        );
        // Both directions fire, and every delivered outbound frame is tapped in
        // order: the tap is in the path, not a fabricated side channel.
        assert!(
            !outbound.is_empty(),
            "at least one outbound frame is tapped"
        );
        assert_eq!(
            outbound, delivered,
            "every delivered outbound frame is tapped, in order"
        );
    }

    struct PanickingDebugSink;

    impl DebugSink for PanickingDebugSink {
        fn emit(&self, _event: DebugEvent) {
            panic!("misbehaving out-of-repo debug sink");
        }
    }

    #[test]
    fn a_panicking_debug_sink_does_not_take_down_the_dispatch() {
        // The trait forbids panicking, but it is `pub`, so a bad out-of-repo sink
        // could. `emit_debug` catches it: `receive_frame` must still succeed.
        let (host_config, product) = runtime_config("myapp.dot");
        let runtime = ProductRuntime::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
            test_spawner(),
            Arc::new(RecordingSink::default()),
        );
        runtime.set_debug_sink(
            ChannelId("myapp.dot".to_string()),
            Arc::new(PanickingDebugSink),
        );

        let ids = subscription_ids("theme_subscribe").expect("known subscription");
        let raw = ProtocolMessage {
            request_id: "theme:1".to_string(),
            payload: Payload {
                id: ids.start_id,
                value: Vec::new(),
            },
        }
        .encode();
        // The inbound tap panics inside receive_frame; the guard swallows it.
        let result = futures::executor::block_on(runtime.receive_frame(raw));
        assert!(
            result.is_ok(),
            "a panicking sink must not fail the dispatch"
        );
    }

    #[test]
    fn frame_direction_wire_str_is_product_vantage() {
        // The wire string is product-vantage (what the debugger and design doc
        // use), the inverse of the enum's host-vantage names: a frame the host
        // tapped as `In` (product to core) *left the product*, so it serializes
        // as `"out"`. This pins the convention so a sink can't re-invert it.
        assert_eq!(FrameDirection::In.wire_str(), "out");
        assert_eq!(FrameDirection::Out.wire_str(), "in");
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
