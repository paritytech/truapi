//! wasm-bindgen surface. Exposes [`WasmTrUApiCore`] to JavaScript hosts so
//! they can wire the TrUAPI core into a browser or worker shell.
//!
//! The browser side hands a `callbacks` object (a `JsBridge`) to the
//! constructor. The bridge implements every host-side capability the
//! [`truapi_platform::Platform`] trait set requires. Internally the bridge
//! is wrapped in a [`SendWrapper`] so it satisfies the `Send` bound the
//! platform trait set imposes; sound on wasm32 because the runtime is
//! single-threaded.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::mpsc;
use futures::stream::{BoxStream, StreamExt};
use js_sys::{Function, Reflect, Uint8Array};
use parity_scale_codec::{Decode, Encode};
use send_wrapper::SendWrapper;
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, Permissions, Storage,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::TrUApiCore;
use crate::frame::ProtocolMessage;
use crate::subscription::Spawner;
use crate::transport::Transport;

/// Bundle of JS-side callbacks the bridge invokes. Names map to camelCase
/// keys on the JS object passed to the constructor.
struct JsBridge {
    navigate_to: Function,
    push_notification: Function,
    device_permission: Function,
    remote_permission: Function,
    feature_supported: Function,
    local_storage_read: Function,
    local_storage_write: Function,
    local_storage_clear: Function,
    /// Optional. Hosts that own JSON-RPC connections (e.g. dotli with its
    /// "smoldot vs RPC node" toggle) provide this; otherwise chain calls
    /// fail with an "unavailable" reason.
    chain_connect: Option<Function>,
    emit_frame: Function,
    dispose: Function,
}

impl JsBridge {
    fn from_js(callbacks: &JsValue) -> Result<Self, JsValue> {
        Ok(Self {
            navigate_to: get_function(callbacks, "navigateTo")?,
            push_notification: get_function(callbacks, "pushNotification")?,
            device_permission: get_function(callbacks, "devicePermission")?,
            remote_permission: get_function(callbacks, "remotePermission")?,
            feature_supported: get_function(callbacks, "featureSupported")?,
            local_storage_read: get_function(callbacks, "localStorageRead")?,
            local_storage_write: get_function(callbacks, "localStorageWrite")?,
            local_storage_clear: get_function(callbacks, "localStorageClear")?,
            chain_connect: get_optional_function(callbacks, "chainConnect")?,
            emit_frame: get_function(callbacks, "emitFrame")?,
            dispose: get_optional_function(callbacks, "dispose")?.unwrap_or_else(noop_function),
        })
    }
}

struct WasmCallbackTransport {
    bridge: SendWrapper<Arc<JsBridge>>,
    disposed: Arc<AtomicBool>,
}

impl Transport for WasmCallbackTransport {
    fn send(&self, message: ProtocolMessage) {
        if self.disposed.load(Ordering::Relaxed) {
            return;
        }
        let frame = Uint8Array::from(message.encode().as_slice());
        if let Err(err) = self.bridge.emit_frame.call1(&JsValue::NULL, &frame) {
            web_sys::console::error_1(&err);
        }
    }

    fn on_message(
        &self,
        _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
    ) -> Box<dyn FnOnce()> {
        Box::new(|| {})
    }
}

struct WasmPlatform {
    bridge: SendWrapper<Arc<JsBridge>>,
}

impl WasmPlatform {
    fn new(bridge: Arc<JsBridge>) -> Self {
        Self {
            bridge: SendWrapper::new(bridge),
        }
    }
}

impl Navigation for WasmPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        invoke_navigate_to(&self.bridge, &url)
            .await
            .map_err(|reason| v01::HostNavigateToError::Unknown { reason })
    }
}

impl Notifications for WasmPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<(), v01::GenericError> {
        invoke_unit(&self.bridge.push_notification, notification.encode())
            .await
            .map_err(generic)
    }
}

impl Permissions for WasmPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        let granted = invoke_bool(&self.bridge.device_permission, request.encode())
            .await
            .map_err(generic)?;
        Ok(v01::HostDevicePermissionResponse { granted })
    }

    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        let granted = invoke_bool(&self.bridge.remote_permission, request.encode())
            .await
            .map_err(generic)?;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

impl Features for WasmPlatform {
    async fn feature_supported(
        &self,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
        let supported = invoke_bool(&self.bridge.feature_supported, request.encode())
            .await
            .map_err(generic)?;
        Ok(HostFeatureSupportedResponse::V1(
            v01::HostFeatureSupportedResponse { supported },
        ))
    }
}

impl Storage for WasmPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        invoke_local_storage_read(&self.bridge, &key)
            .await
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        invoke_local_storage_write(&self.bridge, &key, &value)
            .await
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        invoke_local_storage_clear(&self.bridge, &key)
            .await
            .map_err(|reason| v01::HostLocalStorageReadError::Unknown { reason })
    }
}

impl ChainProvider for WasmPlatform {
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let chain_connect = match self.bridge.chain_connect.clone() {
            Some(f) => f,
            None => {
                return Err(generic(
                    "chainConnect callback not provided by host".to_string(),
                ));
            }
        };
        let chain_connect = SendWrapper::new(chain_connect);
        SendWrapper::new(async move {
            let (response_tx, response_rx) = mpsc::unbounded::<String>();
            let on_response = Closure::wrap(Box::new(move |json: JsValue| {
                let s = json.as_string().unwrap_or_default();
                let _ = response_tx.unbounded_send(s);
            }) as Box<dyn FnMut(JsValue)>);

            let genesis_hex = genesis_hash.iter().fold(
                String::with_capacity(2 + genesis_hash.len() * 2),
                |mut s, b| {
                    use std::fmt::Write;
                    let _ = write!(s, "{b:02x}");
                    s
                },
            );
            let genesis_arg = JsValue::from_str(&format!("0x{genesis_hex}"));
            let returned = chain_connect
                .call2(
                    &JsValue::NULL,
                    &genesis_arg,
                    on_response.as_ref().unchecked_ref(),
                )
                .map_err(|err| generic(js_to_string(err)))?;
            let resolved = await_optional_promise(returned).await.map_err(generic)?;
            if resolved.is_null() || resolved.is_undefined() {
                return Err(generic("chainConnect returned no connection".into()));
            }
            let send_fn = Reflect::get(&resolved, &JsValue::from_str("send"))
                .map_err(|_| generic("chainConnect must return { send, close }".into()))?
                .dyn_into::<Function>()
                .map_err(|_| generic("chainConnect.send must be a function".into()))?;
            let close_fn = Reflect::get(&resolved, &JsValue::from_str("close"))
                .map_err(|_| generic("chainConnect.close must be a function".into()))?
                .dyn_into::<Function>()
                .map_err(|_| generic("chainConnect.close must be a function".into()))?;

            Ok(Box::new(JsCallbackJsonRpcConnection {
                send_fn: SendWrapper::new(send_fn),
                close_fn: SendWrapper::new(close_fn),
                _on_response: SendWrapper::new(on_response),
                response_rx: std::sync::Mutex::new(Some(response_rx)),
            }) as Box<dyn JsonRpcConnection>)
        })
        .await
    }
}

// Account, signing, statement-store, and preimage flows live in the Rust
// core itself. Their `truapi::api::*` trait defaults return `Unsupported`
// until those in-core implementations land. The JS bridge only carries
// callbacks for the platform capabilities the core cannot satisfy alone.

struct JsCallbackJsonRpcConnection {
    send_fn: SendWrapper<Function>,
    close_fn: SendWrapper<Function>,
    /// Closure must outlive the connection so JS keeps a live ref to the
    /// response sink. Dropped together with the rest of the struct.
    _on_response: SendWrapper<Closure<dyn FnMut(JsValue)>>,
    response_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}

impl JsonRpcConnection for JsCallbackJsonRpcConnection {
    fn send(&self, request: String) {
        let arg = JsValue::from_str(&request);
        if let Err(err) = self.send_fn.call1(&JsValue::NULL, &arg) {
            web_sys::console::error_1(&err);
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => futures::stream::empty().boxed(),
        }
    }
}

impl Drop for JsCallbackJsonRpcConnection {
    fn drop(&mut self) {
        let _ = self.close_fn.call0(&JsValue::NULL);
    }
}

fn generic(reason: String) -> v01::GenericError {
    v01::GenericError::GenericError(v01::GenericErr { reason })
}

/// Await the JS callback's return value if it's a Promise; pass other
/// values through unchanged. Every host callback resolves through this so
/// the JS side is free to be sync or async.
async fn await_optional_promise(returned: JsValue) -> Result<JsValue, String> {
    if returned.is_instance_of::<js_sys::Promise>() {
        let promise = returned.unchecked_into::<js_sys::Promise>();
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(js_to_string)
    } else {
        Ok(returned)
    }
}

fn invoke_navigate_to(
    bridge: &JsBridge,
    url: &str,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = bridge.navigate_to.clone();
    let url = url.to_string();
    SendWrapper::new(async move {
        let arg = JsValue::from_str(&url);
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_unit(
    fn_: &Function,
    payload: Vec<u8>,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = Uint8Array::from(payload.as_slice());
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_bool(
    fn_: &Function,
    payload: Vec<u8>,
) -> impl std::future::Future<Output = Result<bool, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = Uint8Array::from(payload.as_slice());
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        Ok(resolved.as_bool().unwrap_or(false))
    })
}

fn invoke_local_storage_read(
    bridge: &JsBridge,
    key: &str,
) -> impl std::future::Future<Output = Result<Option<Vec<u8>>, String>> + Send {
    let fn_ = bridge.local_storage_read.clone();
    let key = key.to_string();
    SendWrapper::new(async move {
        let arg = JsValue::from_str(&key);
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        if resolved.is_null() || resolved.is_undefined() {
            return Ok(None);
        }
        let array = resolved.dyn_into::<Uint8Array>().map_err(|_| {
            "localStorageRead must resolve to Uint8Array, null or undefined".to_string()
        })?;
        Ok(Some(array.to_vec()))
    })
}

fn invoke_local_storage_write(
    bridge: &JsBridge,
    key: &str,
    value: &[u8],
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = bridge.local_storage_write.clone();
    let key = key.to_string();
    let value = value.to_vec();
    SendWrapper::new(async move {
        let key_arg = JsValue::from_str(&key);
        let value_arg = Uint8Array::from(value.as_slice());
        let returned = fn_
            .call2(&JsValue::NULL, &key_arg, &value_arg)
            .map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_local_storage_clear(
    bridge: &JsBridge,
    key: &str,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = bridge.local_storage_clear.clone();
    let key = key.to_string();
    SendWrapper::new(async move {
        let arg = JsValue::from_str(&key);
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn js_to_string(value: JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|err| err.message().into())
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

fn get_function(callbacks: &JsValue, name: &str) -> Result<Function, JsValue> {
    let value = Reflect::get(callbacks, &JsValue::from_str(name))?;
    value
        .dyn_into::<Function>()
        .map_err(|_| JsValue::from_str(&format!("callbacks.{name} must be a function")))
}

fn get_optional_function(callbacks: &JsValue, name: &str) -> Result<Option<Function>, JsValue> {
    let value = Reflect::get(callbacks, &JsValue::from_str(name))?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .dyn_into::<Function>()
        .map(Some)
        .map_err(|_| JsValue::from_str(&format!("callbacks.{name} must be a function")))
}

fn noop_function() -> Function {
    Function::new_no_args("")
}

struct WasmCoreInner {
    core: TrUApiCore,
    transport: Arc<WasmCallbackTransport>,
    dispose_fn: SendWrapper<Function>,
    disposed: Cell<bool>,
    disposing: Cell<bool>,
}

/// Toggle [`crate::debug_log`] output. Hosts read their `truapi:debug`
/// flag (web: localStorage) and call this once during boot.
#[wasm_bindgen(js_name = setDebugEnabled)]
pub fn set_debug_enabled(enabled: bool) {
    crate::debug_log::set_enabled(enabled);
}

/// JS-callable handle to the TrUAPI core. Constructed once per shell boot.
#[wasm_bindgen]
pub struct WasmTrUApiCore {
    inner: Rc<WasmCoreInner>,
}

#[wasm_bindgen]
impl WasmTrUApiCore {
    /// Build the core from a JS callbacks object. The object must define
    /// every host capability the [`truapi_platform::Platform`] trait set
    /// requires (camelCase property names; see the source for the full
    /// list).
    #[wasm_bindgen(constructor)]
    pub fn new(callbacks: JsValue) -> Result<WasmTrUApiCore, JsValue> {
        let bridge = Arc::new(JsBridge::from_js(&callbacks)?);
        let disposed = Arc::new(AtomicBool::new(false));
        let transport = Arc::new(WasmCallbackTransport {
            bridge: SendWrapper::new(bridge.clone()),
            disposed: disposed.clone(),
        });
        let dispose_fn = SendWrapper::new(bridge.dispose.clone());
        let platform = Arc::new(WasmPlatform::new(bridge));
        let spawner: Spawner = Arc::new(|fut| {
            wasm_bindgen_futures::spawn_local(fut);
        });
        let core = TrUApiCore::from_platform(platform, spawner);
        Ok(Self {
            inner: Rc::new(WasmCoreInner {
                core,
                transport,
                dispose_fn,
                disposed: Cell::new(false),
                disposing: Cell::new(false),
            }),
        })
    }

    /// Push a SCALE-encoded protocol frame into the dispatcher. Responses
    /// (and subscription items) flow back through the `emitFrame`
    /// callback.
    #[wasm_bindgen(js_name = receiveFromProduct)]
    pub async fn receive_from_product(&self, frame: Vec<u8>) -> Result<(), JsValue> {
        if self.inner.disposed.get() {
            return Ok(());
        }

        let message = ProtocolMessage::decode(&mut &*frame)
            .map_err(|err| JsValue::from_str(&format!("invalid frame: {err}")))?;

        let transport: Arc<dyn Transport> = self.inner.transport.clone();
        self.inner.core.dispatch(message, transport).await;
        Ok(())
    }

    /// Tear down the bridge. Invokes the JS-side `dispose` callback so the
    /// host can drop its end of the wiring.
    pub fn dispose(&self) -> Result<(), JsValue> {
        if self.inner.disposing.replace(true) {
            return Ok(());
        }

        self.inner.transport.disposed.store(true, Ordering::Relaxed);

        let result = self.inner.dispose_fn.call0(&JsValue::NULL).map(|_| ());

        self.inner.disposed.set(true);
        self.inner.disposing.set(false);
        result
    }

    /// Push the currently-paired session into the core. Called by the
    /// host shell whenever the user pairs / unpairs. `pubkey` must be
    /// exactly 32 bytes (sr25519 root public key); usernames may be
    /// null / undefined when the identity record carries no value.
    #[wasm_bindgen(js_name = setActiveSession)]
    pub fn set_active_session(
        &self,
        pubkey: Vec<u8>,
        lite_username: Option<String>,
        full_username: Option<String>,
    ) -> Result<(), JsValue> {
        let public_key: [u8; 32] = pubkey.as_slice().try_into().map_err(|_| {
            JsValue::from_str(&format!(
                "setActiveSession: pubkey must be 32 bytes, got {}",
                pubkey.len()
            ))
        })?;
        self.inner
            .core
            .session_state()
            .set_session(crate::host_logic::session::SessionInfo {
                public_key,
                lite_username,
                full_username,
            });
        Ok(())
    }

    /// Drop the currently-paired session.
    #[wasm_bindgen(js_name = clearActiveSession)]
    pub fn clear_active_session(&self) {
        self.inner.core.session_state().clear_session();
    }
}
