//! wasm-bindgen surface. Exposes [`WasmTrUApiCore`] to JavaScript hosts so
//! they can wire the TrUAPI core into a browser or worker shell.
//!
//! The browser side hands a `callbacks` object (a `JsBridge`) to the
//! constructor. The bridge implements every host-side capability the
//! [`truapi_platform::Platform`] trait set requires. Internally the bridge
//! is wrapped in a [`SendWrapper`] so it satisfies the `Send` bound the
//! platform trait set imposes; sound on wasm32 because the runtime is
//! single-threaded.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::{AbortHandle, Abortable};
use futures::stream::{self, BoxStream, Stream, StreamExt};
use js_sys::{Function, Reflect, Uint8Array};
use parity_scale_codec::{Decode, Encode};
use send_wrapper::SendWrapper;
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, PairingDeeplinkScheme,
    PairingPresenter, Permissions, PreimageHost, RuntimeConfig, RuntimeConfigValidationError,
    SessionStore, SessionUiInfo, Storage, ThemeHost, UserConfirmation,
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
    confirm_sign_payload: Option<Function>,
    confirm_sign_raw: Option<Function>,
    confirm_create_transaction: Option<Function>,
    confirm_account_alias: Option<Function>,
    confirm_resource_allocation: Option<Function>,
    confirm_preimage_submit: Option<Function>,
    submit_preimage: Option<Function>,
    lookup_preimage: Option<Function>,
    subscribe_theme: Option<Function>,
    present_pairing: Option<Function>,
    read_session: Option<Function>,
    write_session: Option<Function>,
    clear_session: Option<Function>,
    subscribe_session_store: Option<Function>,
    session_ui_changed: Option<Function>,
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
            confirm_sign_payload: get_optional_function(callbacks, "confirmSignPayload")?,
            confirm_sign_raw: get_optional_function(callbacks, "confirmSignRaw")?,
            confirm_create_transaction: get_optional_function(
                callbacks,
                "confirmCreateTransaction",
            )?,
            confirm_account_alias: get_optional_function(callbacks, "confirmAccountAlias")?,
            confirm_resource_allocation: get_optional_function(
                callbacks,
                "confirmResourceAllocation",
            )?,
            confirm_preimage_submit: get_optional_function(callbacks, "confirmPreimageSubmit")?,
            submit_preimage: get_optional_function(callbacks, "submitPreimage")?,
            lookup_preimage: get_optional_function(callbacks, "preimageLookupSubscribe")?,
            subscribe_theme: get_optional_function(callbacks, "themeSubscribe")?,
            present_pairing: get_optional_function(callbacks, "presentPairing")?,
            read_session: get_optional_function(callbacks, "readSession")?,
            write_session: get_optional_function(callbacks, "writeSession")?,
            clear_session: get_optional_function(callbacks, "clearSession")?,
            subscribe_session_store: get_optional_function(callbacks, "subscribeSessionStore")?,
            session_ui_changed: get_optional_function(callbacks, "sessionUiChanged")?,
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

            let genesis_arg = JsValue::from_str(&format!("0x{}", hex::encode(&genesis_hash)));
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
    async fn present_pairing(&self, deeplink: String) -> Result<(), v01::GenericError> {
        let Some(fn_) = self.bridge.present_pairing.as_ref() else {
            return Err(v01::GenericError {
                reason: "presentPairing callback not provided by host".to_string(),
            });
        };
        invoke_string_unit(fn_, deeplink).await.map_err(generic)
    }
}

impl SessionStore for WasmPlatform {
    async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
        let Some(fn_) = self.bridge.read_session.as_ref() else {
            return Ok(None);
        };
        invoke_optional_bytes(fn_).await.map_err(generic)
    }

    async fn write_session(&self, value: Vec<u8>) -> Result<(), v01::GenericError> {
        let Some(fn_) = self.bridge.write_session.as_ref() else {
            return Ok(());
        };
        invoke_bytes_unit(fn_, value).await.map_err(generic)
    }

    async fn clear_session(&self) -> Result<(), v01::GenericError> {
        let Some(fn_) = self.bridge.clear_session.as_ref() else {
            return Ok(());
        };
        invoke_no_args_unit(fn_).await.map_err(generic)
    }

    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        let Some(fn_) = self.bridge.subscribe_session_store.as_ref() else {
            return stream::once(async { Ok(()) }).boxed();
        };
        invoke_js_subscription(fn_, None, parse_session_store_tick).boxed()
    }

    fn session_ui_changed(&self, info: SessionUiInfo) {
        let Some(fn_) = self.bridge.session_ui_changed.as_ref() else {
            return;
        };
        if let Err(err) = fn_.call1(&JsValue::NULL, &session_ui_info_to_js(&info)) {
            web_sys::console::error_1(&err);
        }
    }
}

impl UserConfirmation for WasmPlatform {
    async fn confirm_sign_payload(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        let Some(fn_) = self.bridge.confirm_sign_payload.as_ref() else {
            return Ok(false);
        };
        invoke_bool(fn_, review).await.map_err(generic)
    }

    async fn confirm_sign_raw(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        let Some(fn_) = self.bridge.confirm_sign_raw.as_ref() else {
            return Ok(false);
        };
        invoke_bool(fn_, review).await.map_err(generic)
    }

    async fn confirm_create_transaction(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        let Some(fn_) = self.bridge.confirm_create_transaction.as_ref() else {
            return Ok(false);
        };
        invoke_bool(fn_, review).await.map_err(generic)
    }

    async fn confirm_account_alias(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        let Some(fn_) = self.bridge.confirm_account_alias.as_ref() else {
            return Ok(false);
        };
        invoke_bool(fn_, review).await.map_err(generic)
    }

    async fn confirm_resource_allocation(
        &self,
        review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        let Some(fn_) = self.bridge.confirm_resource_allocation.as_ref() else {
            return Ok(false);
        };
        invoke_bool(fn_, review).await.map_err(generic)
    }
}

impl ThemeHost for WasmPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        let Some(fn_) = self.bridge.subscribe_theme.as_ref() else {
            return stream::once(async { Ok(v01::ThemeVariant::Dark) }).boxed();
        };
        invoke_js_subscription(fn_, None, parse_theme_item).boxed()
    }
}

impl PreimageHost for WasmPlatform {
    async fn confirm_preimage_submit(&self, size: u64) -> Result<(), v01::PreimageSubmitError> {
        let Some(fn_) = self.bridge.confirm_preimage_submit.as_ref() else {
            return Err(v01::PreimageSubmitError::Unknown {
                reason: "confirmPreimageSubmit callback not provided by host".to_string(),
            });
        };
        invoke_u64_unit(fn_, size)
            .await
            .map_err(|reason| v01::PreimageSubmitError::Unknown { reason })
    }

    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        let Some(fn_) = self.bridge.submit_preimage.as_ref() else {
            return Err(v01::PreimageSubmitError::Unknown {
                reason: "submitPreimage callback not provided by host".to_string(),
            });
        };
        invoke_bytes_return(fn_, value)
            .await
            .map_err(|reason| v01::PreimageSubmitError::Unknown { reason })
    }

    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let Some(fn_) = self.bridge.lookup_preimage.as_ref() else {
            return stream::once(async { Ok(None) }).boxed();
        };
        invoke_js_subscription(fn_, Some(key), parse_preimage_lookup_item).boxed()
    }
}

// Account, signing, and statement-store flows live in the Rust
// core itself. Their `truapi::api::*` trait defaults return `Unsupported`
// until those in-core implementations land. The JS bridge only carries
// callbacks for the platform capabilities the core cannot satisfy alone.

struct JsSubscriptionStream<T> {
    rx: mpsc::UnboundedReceiver<T>,
    _send_item: SendWrapper<Closure<dyn FnMut(JsValue)>>,
    dispose: Option<SendWrapper<Function>>,
}

impl<T> Stream for JsSubscriptionStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_next(cx)
    }
}

impl<T> Drop for JsSubscriptionStream<T> {
    fn drop(&mut self) {
        if let Some(dispose) = self.dispose.take() {
            let _ = dispose.call0(&JsValue::NULL);
        }
    }
}

fn invoke_js_subscription<T>(
    fn_: &Function,
    payload: Option<Vec<u8>>,
    parse_item: fn(JsValue) -> Result<T, String>,
) -> BoxStream<'static, Result<T, v01::GenericError>>
where
    T: Send + 'static,
{
    let (tx, rx) = mpsc::unbounded::<Result<T, v01::GenericError>>();
    let send_item = Closure::wrap(Box::new(move |value: JsValue| {
        let item = parse_item(value).map_err(generic);
        let _ = tx.unbounded_send(item);
    }) as Box<dyn FnMut(JsValue)>);

    let call_result = match payload {
        Some(payload) => {
            let arg = Uint8Array::from(payload.as_slice());
            fn_.call2(&JsValue::NULL, &arg, send_item.as_ref().unchecked_ref())
        }
        None => fn_.call1(&JsValue::NULL, send_item.as_ref().unchecked_ref()),
    };

    let dispose = match call_result {
        Ok(value) if value.is_null() || value.is_undefined() => None,
        Ok(value) => match value.dyn_into::<Function>() {
            Ok(dispose) => Some(SendWrapper::new(dispose)),
            Err(_) => {
                return stream::once(async {
                    Err(generic(
                        "subscription callback must return a dispose function, null, or undefined"
                            .to_string(),
                    ))
                })
                .boxed();
            }
        },
        Err(err) => return stream::once(async { Err(generic(js_to_string(err))) }).boxed(),
    };

    Box::pin(JsSubscriptionStream {
        rx,
        _send_item: SendWrapper::new(send_item),
        dispose,
    })
}

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

fn invoke_u64_unit(
    fn_: &Function,
    value: u64,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
        if value > MAX_SAFE_INTEGER {
            return Err("callback numeric argument exceeds JS safe integer range".to_string());
        }
        let arg = JsValue::from_f64(value as f64);
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_string_unit(
    fn_: &Function,
    value: String,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = JsValue::from_str(&value);
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_bytes_return(
    fn_: &Function,
    value: Vec<u8>,
) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = Uint8Array::from(value.as_slice());
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        resolved
            .dyn_into::<Uint8Array>()
            .map(|array| array.to_vec())
            .map_err(|_| "callback must resolve to Uint8Array".to_string())
    })
}

fn invoke_bytes_unit(
    fn_: &Function,
    value: Vec<u8>,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = Uint8Array::from(value.as_slice());
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn parse_preimage_lookup_item(value: JsValue) -> Result<Option<Vec<u8>>, String> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .dyn_into::<Uint8Array>()
        .map(|array| Some(array.to_vec()))
        .map_err(|_| "preimage lookup item must be Uint8Array, null, or undefined".to_string())
}

fn parse_theme_item(value: JsValue) -> Result<v01::ThemeVariant, String> {
    if let Some(theme) = value.as_string() {
        return match theme.as_str() {
            "Light" | "light" => Ok(v01::ThemeVariant::Light),
            "Dark" | "dark" => Ok(v01::ThemeVariant::Dark),
            _ => Err("theme item string must be Light or Dark".to_string()),
        };
    }
    if let Some(theme) = value.as_f64() {
        return match theme as u8 {
            0 if theme == 0.0 => Ok(v01::ThemeVariant::Light),
            1 if theme == 1.0 => Ok(v01::ThemeVariant::Dark),
            _ => Err("theme item number must be 0 or 1".to_string()),
        };
    }
    value
        .dyn_into::<Uint8Array>()
        .map_err(|_| "theme item must be Light, Dark, 0, 1, or encoded ThemeVariant".to_string())
        .and_then(|array| {
            v01::ThemeVariant::decode(&mut array.to_vec().as_slice())
                .map_err(|_| "encoded ThemeVariant item did not decode".to_string())
        })
}

fn parse_session_store_tick(_value: JsValue) -> Result<(), String> {
    Ok(())
}

/// Plain JS object mirroring the generated `SessionUiInfo` TS interface:
/// `connected` is always present, the optional fields only when `Some`.
fn session_ui_info_to_js(info: &SessionUiInfo) -> JsValue {
    let object = js_sys::Object::new();
    let set = |key: &str, value: &JsValue| {
        let _ = Reflect::set(&object, &JsValue::from_str(key), value);
    };
    set("connected", &JsValue::from_bool(info.connected));
    if let Some(public_key) = &info.public_key {
        set("publicKey", &Uint8Array::from(public_key.as_slice()));
    }
    if let Some(identity_account_id) = &info.identity_account_id {
        set(
            "identityAccountId",
            &Uint8Array::from(identity_account_id.as_slice()),
        );
    }
    if let Some(lite_username) = &info.lite_username {
        set("liteUsername", &JsValue::from_str(lite_username));
    }
    if let Some(full_username) = &info.full_username {
        set("fullUsername", &JsValue::from_str(full_username));
    }
    object.into()
}

fn invoke_no_args_unit(
    fn_: &Function,
) -> impl std::future::Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = fn_.call0(&JsValue::NULL).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_optional_bytes(
    fn_: &Function,
) -> impl std::future::Future<Output = Result<Option<Vec<u8>>, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = fn_.call0(&JsValue::NULL).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        if resolved.is_null() || resolved.is_undefined() {
            return Ok(None);
        }
        let array = resolved
            .dyn_into::<Uint8Array>()
            .map_err(|_| "callback must resolve to Uint8Array, null or undefined".to_string())?;
        Ok(Some(array.to_vec()))
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
        return Err(JsValue::from_str("runtimeConfig is required"));
    }

    let config = RuntimeConfig {
        product_label: get_required_string(value, "productLabel")?,
        product_id: get_required_string(value, "productId")?,
        site_id: get_required_string(value, "siteId")?,
        host_name: get_required_string(value, "hostName")?,
        host_icon: get_optional_string(value, "hostIcon")?,
        host_version: get_optional_string(value, "hostVersion")?,
        platform_type: get_optional_string(value, "platformType")?,
        platform_version: get_optional_string(value, "platformVersion")?,
        people_chain_genesis_hash: get_required_bytes32(value, "peopleChainGenesisHash")?,
        pairing_deeplink_scheme: {
            let scheme = get_required_string(value, "pairingDeeplinkScheme")?;
            match scheme.as_str() {
                "polkadotapp" | "polkadotApp" | "PolkadotApp" => PairingDeeplinkScheme::PolkadotApp,
                "polkadotappdev" | "polkadotAppDev" | "PolkadotAppDev" => {
                    PairingDeeplinkScheme::PolkadotAppDev
                }
                other => {
                    return Err(JsValue::from_str(&format!(
                        "runtimeConfig.pairingDeeplinkScheme has unsupported value {other:?}"
                    )));
                }
            }
        },
    };
    config.validate().map_err(runtime_config_validation_to_js)?;
    Ok(config)
}

fn runtime_config_validation_to_js(err: RuntimeConfigValidationError) -> JsValue {
    match err {
        RuntimeConfigValidationError::EmptyField { field } => JsValue::from_str(&format!(
            "runtimeConfig.{} must not be empty",
            runtime_config_field_to_js(field)
        )),
        RuntimeConfigValidationError::InvalidHostIcon { reason } => JsValue::from_str(&format!(
            "runtimeConfig.hostIcon must be an absolute HTTPS URL: {reason}"
        )),
        RuntimeConfigValidationError::InsecureHostIcon { scheme } => JsValue::from_str(&format!(
            "runtimeConfig.hostIcon must use https scheme, got {scheme:?}"
        )),
    }
}

fn runtime_config_field_to_js(field: &str) -> &str {
    match field {
        "product_label" => "productLabel",
        "product_id" => "productId",
        "site_id" => "siteId",
        "host_name" => "hostName",
        "people_chain_genesis_hash" => "peopleChainGenesisHash",
        other => other,
    }
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

fn get_required_string(value: &JsValue, name: &str) -> Result<String, JsValue> {
    get_optional_string(value, name)?
        .ok_or_else(|| JsValue::from_str(&format!("runtimeConfig.{name} is required")))
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

fn get_required_bytes32(value: &JsValue, name: &str) -> Result<[u8; 32], JsValue> {
    get_optional_bytes32(value, name)?
        .ok_or_else(|| JsValue::from_str(&format!("runtimeConfig.{name} is required")))
}

fn parse_hex32(value: &str) -> Result<[u8; 32], String> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 {
        return Err(format!(
            "expected 32-byte hex string, got {} hex chars",
            raw.len()
        ));
    }
    let bytes = hex::decode(raw).map_err(|_| "invalid hex".to_string())?;
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("expected 32 bytes, got {}", bytes.len()))
}

struct WasmCoreInner {
    core: TrUApiCore,
    transport: Arc<WasmCallbackTransport>,
    dispose_fn: SendWrapper<Function>,
    disposed: Cell<bool>,
    disposing: Cell<bool>,
    /// Abort handles for in-flight `receive_from_product` dispatches, keyed
    /// by a local counter. `dispose` aborts them all so long-pending handlers
    /// unwind instead of outliving the core.
    in_flight: RefCell<HashMap<u64, AbortHandle>>,
    next_dispatch_id: Cell<u64>,
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`).
/// Hosts read their `truapi:logLevel` flag (web: localStorage) and call this
/// during boot, or again at any time to re-tune verbosity.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn set_log_level(level: &str) {
    crate::logging::set_level(crate::logging::parse_level(level));
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
        crate::logging::init();
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
                in_flight: RefCell::new(HashMap::new()),
                next_dispatch_id: Cell::new(0),
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
        // Register the dispatch so `dispose` can abort it; a long-pending
        // handler then unwinds instead of outliving the core.
        let dispatch_id = self.inner.next_dispatch_id.get();
        self.inner.next_dispatch_id.set(dispatch_id.wrapping_add(1));
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        self.inner
            .in_flight
            .borrow_mut()
            .insert(dispatch_id, abort_handle);
        let _ = Abortable::new(
            self.inner.core.dispatch(message, transport),
            abort_registration,
        )
        .await;
        self.inner.in_flight.borrow_mut().remove(&dispatch_id);
        Ok(())
    }

    /// Tear down the bridge. Invokes the JS-side `dispose` callback so the
    /// host can drop its end of the wiring.
    pub fn dispose(&self) -> Result<(), JsValue> {
        if self.inner.disposing.replace(true) {
            return Ok(());
        }

        self.inner.transport.disposed.store(true, Ordering::Relaxed);
        for (_, handle) in self.inner.in_flight.borrow_mut().drain() {
            handle.abort();
        }

        let result = self.inner.dispose_fn.call0(&JsValue::NULL).map(|_| ());

        self.inner.disposed.set(true);
        self.inner.disposing.set(false);
        result
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    #[wasm_bindgen(js_name = disconnect)]
    pub async fn disconnect(&self) -> Result<(), JsValue> {
        self.inner.core.disconnect_async().await;
        Ok(())
    }
}
