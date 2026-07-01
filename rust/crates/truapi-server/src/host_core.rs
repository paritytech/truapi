//! Stable host-embedding API for the TrUAPI server runtime.
//!
//! `HostCore` is the target-neutral boundary embedders should use. Platform
//! adapters provide a [`truapi_platform::Platform`] implementation, a task
//! [`Spawner`], and a [`FrameSink`] for outgoing protocol frames. Target-specific
//! shells such as wasm-bindgen, iOS FFI, or desktop IPC should keep their
//! conversion code outside this module.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::{AbortHandle, Abortable};
use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
use tracing::instrument;
use truapi::v01;
use truapi_platform::{
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform, RuntimeConfig,
};

use crate::core::TrUApiCore;
use crate::frame::ProtocolMessage;
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

/// Errors returned by [`HostCore::receive_frame`].
#[derive(Debug, Error)]
pub enum HostCoreError {
    /// Incoming bytes did not decode as a protocol frame.
    #[error("invalid frame: {reason}")]
    InvalidFrame {
        /// Decode failure reason.
        reason: String,
    },
}

/// Target-neutral host runtime wrapper.
///
/// `HostCore` owns the dispatcher/runtime core and handles byte-frame ingress,
/// response/subscription egress, in-flight dispatch cancellation on dispose,
/// and core-owned auth/session lifecycle operations.
pub struct HostCore {
    core: TrUApiCore,
    transport: Arc<SinkTransport>,
    disposed: Arc<AtomicBool>,
    in_flight: Mutex<HashMap<u64, AbortHandle>>,
    next_dispatch_id: AtomicU64,
}

impl HostCore {
    /// Build a host core around a platform implementation and outgoing frame
    /// sink.
    #[instrument(skip_all, fields(runtime.method = "host_core.from_platform_with_config"))]
    pub fn from_platform_with_config<P>(
        platform: Arc<P>,
        runtime_config: RuntimeConfig,
        spawner: Spawner,
        sink: Arc<dyn FrameSink>,
    ) -> Self
    where
        P: Platform + 'static,
    {
        let disposed = Arc::new(AtomicBool::new(false));
        let transport = Arc::new(SinkTransport {
            sink,
            disposed: disposed.clone(),
        });
        Self {
            core: TrUApiCore::from_platform_with_config(platform, runtime_config, spawner),
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
    #[instrument(skip_all, fields(runtime.method = "host_core.receive_frame"))]
    pub async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), HostCoreError> {
        if self.disposed.load(Ordering::Acquire) {
            return Ok(());
        }

        let message = ProtocolMessage::decode(&mut frame.as_slice()).map_err(|err| {
            HostCoreError::InvalidFrame {
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
        Ok(())
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    #[instrument(skip_all, fields(runtime.method = "host_core.disconnect_session"))]
    pub async fn disconnect_session(&self) {
        self.core.disconnect_async().await;
    }

    /// Cancel an in-flight pairing request. No-op when no pairing is active.
    #[instrument(skip_all, fields(runtime.method = "host_core.cancel_pairing"))]
    pub fn cancel_pairing(&self) {
        self.core.cancel_login();
    }

    /// Notify the core that the host-global auth session slot may have changed.
    /// The core re-reads the persisted blob and emits any resulting
    /// session/auth state changes.
    #[instrument(skip_all, fields(runtime.method = "host_core.notify_session_store_changed"))]
    pub fn notify_session_store_changed(&self) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }
        self.core.notify_session_store_changed();
    }

    /// Read a stored permission authorization status without prompting.
    #[instrument(skip_all, fields(runtime.method = "host_core.permission_authorization_status"))]
    pub async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        self.core.permission_authorization_status(request).await
    }

    /// Read stored permission authorization statuses without prompting.
    #[instrument(skip_all, fields(runtime.method = "host_core.permission_authorization_statuses"))]
    pub async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        self.core.permission_authorization_statuses(requests).await
    }

    /// Update a stored permission authorization status. `NotDetermined`
    /// clears the stored value so the next product request prompts again.
    #[instrument(skip_all, fields(runtime.method = "host_core.set_permission_authorization_status"))]
    pub async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        self.core
            .set_permission_authorization_status(request, status)
            .await
    }

    /// Dispose this host core. Idempotent.
    ///
    /// Disposal suppresses future outgoing frames and aborts in-flight dispatch
    /// futures. Adapter-specific resource cleanup remains the adapter's
    /// responsibility.
    #[instrument(skip_all, fields(runtime.method = "host_core.dispose"))]
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
