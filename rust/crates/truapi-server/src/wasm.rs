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
use futures::stream::{self, BoxStream, StreamExt};
use js_sys::{Function, Reflect, Uint8Array};
use parity_scale_codec::{Decode, Encode};
use send_wrapper::SendWrapper;
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, PairingDeeplinkScheme,
    PairingPresenter, Permissions, PreimageHost, RuntimeConfig, SessionStore, Storage, ThemeHost,
    UserConfirmation,
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
    cancel_notification: Option<Function>,
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
            cancel_notification: get_optional_function(callbacks, "cancelNotification")?,
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
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        let id = invoke_u32(&self.bridge.push_notification, notification.encode())
            .await
            .map_err(generic)?;
        Ok(v01::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: v01::NotificationId) -> Result<(), v01::GenericError> {
        let Some(fn_) = self.bridge.cancel_notification.as_ref() else {
            return Ok(());
        };
        invoke_u32_unit(fn_, id).await.map_err(generic)
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
                // The host must hand back JSON-RPC frames as strings. Drop (and
                // log) non-string values rather than forwarding an empty frame
                // that would desync request/response correlation.
                match json.as_string() {
                    Some(s) => {
                        let _ = response_tx.unbounded_send(s);
                    }
                    None => web_sys::console::error_1(&JsValue::from_str(
                        "chainConnect onResponse expected a JSON string; dropping non-string value",
                    )),
                }
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

impl PairingPresenter for WasmPlatform {
    async fn present_pairing(&self, _deeplink: String) -> Result<(), v01::GenericError> {
        Err(v01::GenericError {
            reason: "pairing presenter callback not provided by host".to_string(),
        })
    }
}

impl SessionStore for WasmPlatform {
    async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(None)
    }

    async fn write_session(&self, _value: Vec<u8>) -> Result<(), v01::GenericError> {
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), v01::GenericError> {
        Ok(())
    }

    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        stream::once(async { Ok(()) }).boxed()
    }
}

impl UserConfirmation for WasmPlatform {
    async fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
        Ok(false)
    }

    async fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
        Ok(false)
    }

    async fn confirm_create_transaction(
        &self,
        _review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        Ok(false)
    }

    async fn confirm_resource_allocation(
        &self,
        _review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        Ok(false)
    }
}

impl ThemeHost for WasmPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::Theme, v01::GenericError>> {
        stream::empty().boxed()
    }
}

impl PreimageHost for WasmPlatform {
    async fn confirm_preimage_submit(&self, _size: u64) -> Result<(), v01::PreimageSubmitError> {
        Err(v01::PreimageSubmitError::Unknown {
            reason: "preimage confirmation callback not provided by host".to_string(),
        })
    }

    async fn submit_preimage(&self, _value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        Err(v01::PreimageSubmitError::Unknown {
            reason: "preimage submit callback not provided by host".to_string(),
        })
    }

    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        stream::empty().boxed()
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

    /// Single-take: the response receiver is handed out exactly once. A second
    /// call yields an empty stream (and logs), since the channel has one
    /// consumer.
    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => {
                web_sys::console::error_1(&JsValue::from_str(
                    "JsCallbackJsonRpcConnection::responses() called more than once",
                ));
                futures::stream::empty().boxed()
            }
        }
    }
}

impl Drop for JsCallbackJsonRpcConnection {
    fn drop(&mut self) {
        let _ = self.close_fn.call0(&JsValue::NULL);
    }
}

fn generic(reason: String) -> v01::GenericError {
    v01::GenericError { reason }
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

fn invoke_bool(
    fn_: &Function,
    payload: Vec<u8>,
) -> impl std::future::Future<Output = Result<bool, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = Uint8Array::from(payload.as_slice());
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        // A non-boolean resolved value is a host contract violation; surface it
        // rather than silently masking it as `false` (which would read as a
        // denial / unsupported and hide the host bug).
        resolved
            .as_bool()
            .ok_or_else(|| "callback must resolve to a boolean".to_string())
    })
}

fn invoke_u32(
    fn_: &Function,
    payload: Vec<u8>,
) -> impl std::future::Future<Output = Result<u32, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = Uint8Array::from(payload.as_slice());
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        let n = resolved
            .as_f64()
            .ok_or_else(|| "callback must resolve to a u32 notification id".to_string())?;
        if n.is_finite() && n >= 0.0 && n <= f64::from(u32::MAX) && n.fract() == 0.0 {
            Ok(n as u32)
        } else {
            Err("callback must resolve to a u32 notification id".to_string())
        }
    })
}

fn invoke_u32_unit(
    fn_: &Function,
    value: u32,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = JsValue::from_f64(f64::from(value));
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
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

fn runtime_config_from_js(value: &JsValue) -> Result<RuntimeConfig, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(RuntimeConfig::compatibility_default());
    }

    let mut config = RuntimeConfig::compatibility_default();
    if let Some(product_label) = get_optional_string(value, "productLabel")? {
        config.product_label = product_label;
    }
    if let Some(product_id) = get_optional_string(value, "productId")? {
        config.product_id = product_id;
    }
    if let Some(site_id) = get_optional_string(value, "siteId")? {
        config.site_id = site_id;
    }
    if let Some(host_metadata_url) = get_optional_string(value, "hostMetadataUrl")? {
        config.host_metadata_url = host_metadata_url;
    }
    if let Some(hash) = get_optional_bytes32(value, "peopleChainGenesisHash")? {
        config.people_chain_genesis_hash = hash;
    }
    if let Some(scheme) = get_optional_string(value, "pairingDeeplinkScheme")? {
        config.pairing_deeplink_scheme = match scheme.as_str() {
            "polkadotapp" | "polkadotApp" | "PolkadotApp" => PairingDeeplinkScheme::PolkadotApp,
            "polkadotappdev" | "polkadotAppDev" | "PolkadotAppDev" => {
                PairingDeeplinkScheme::PolkadotAppDev
            }
            other => {
                return Err(JsValue::from_str(&format!(
                    "runtimeConfig.pairingDeeplinkScheme has unsupported value {other:?}"
                )));
            }
        };
    }
    Ok(config)
}

fn get_optional_string(value: &JsValue, name: &str) -> Result<Option<String>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    property
        .as_string()
        .map(Some)
        .ok_or_else(|| JsValue::from_str(&format!("runtimeConfig.{name} must be a string")))
}

fn get_optional_bytes32(value: &JsValue, name: &str) -> Result<Option<[u8; 32]>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    if let Some(hex) = property.as_string() {
        return parse_hex32(&hex)
            .map(Some)
            .map_err(|reason| JsValue::from_str(&format!("runtimeConfig.{name}: {reason}")));
    }
    let array = property.dyn_into::<Uint8Array>().map_err(|_| {
        JsValue::from_str(&format!("runtimeConfig.{name} must be hex or Uint8Array"))
    })?;
    let bytes = array.to_vec();
    bytes.try_into().map(Some).map_err(|bytes: Vec<u8>| {
        JsValue::from_str(&format!(
            "runtimeConfig.{name} must be exactly 32 bytes, got {}",
            bytes.len()
        ))
    })
}

fn parse_hex32(value: &str) -> Result<[u8; 32], String> {
    let hex = value.strip_prefix("0x").unwrap_or(value);
    if hex.len() != 64 {
        return Err(format!(
            "expected 32-byte hex string, got {} hex chars",
            hex.len()
        ));
    }
    let mut out = [0u8; 32];
    for (idx, byte) in out.iter_mut().enumerate() {
        let start = idx * 2;
        *byte = u8::from_str_radix(&hex[start..start + 2], 16)
            .map_err(|_| "invalid hex".to_string())?;
    }
    Ok(out)
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
    pub fn new(callbacks: JsValue, runtime_config: JsValue) -> Result<WasmTrUApiCore, JsValue> {
        // Surface Rust panics to the browser console. A panic mid-dispatch
        // aborts the call as a wasm trap; the host should treat a thrown error
        // from `receiveFromProduct` as a fatal-instance signal and rebuild the
        // core rather than continue using it.
        console_error_panic_hook::set_once();
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
        let runtime_config = runtime_config_from_js(&runtime_config)?;
        let core = TrUApiCore::from_platform_with_config(platform, runtime_config, spawner);
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
                entropy_secret: None,
                lite_username,
                full_username,
            });
        Ok(())
    }

    /// Attach the host-papp session `ssSecret` used by current dotli entropy
    /// derivation. Returns false when no active session has been pushed yet.
    #[wasm_bindgen(js_name = setActiveSessionEntropySecret)]
    pub fn set_active_session_entropy_secret(&self, secret: Vec<u8>) -> bool {
        self.inner.core.session_state().set_entropy_secret(secret)
    }

    /// Drop the currently-paired session.
    #[wasm_bindgen(js_name = clearActiveSession)]
    pub fn clear_active_session(&self) {
        self.inner.core.session_state().clear_session();
    }
}
