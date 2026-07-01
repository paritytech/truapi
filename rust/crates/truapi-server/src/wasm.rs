//! wasm-bindgen surface. Exposes [`WasmHostCore`] to JavaScript hosts so
//! they can wire the TrUAPI core into a browser or worker shell.
//!
//! The browser side hands a `callbacks` object (a `JsBridge`) to the
//! constructor. The bridge implements every host-side capability the
//! [`truapi_platform::Platform`] trait set requires. Internally the bridge
//! is wrapped in a [`SendWrapper`] so it satisfies the `Send` bound the
//! platform trait set imposes; sound on wasm32 because the runtime is
//! single-threaded.

use core::cell::Cell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, Stream, StreamExt};
use js_sys::{Array, Function, Reflect, Uint8Array};
use parity_scale_codec::{Decode, Encode};
use send_wrapper::SendWrapper;
use truapi::v01;
use truapi_platform::{
    AuthPresenter, AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features, HostInfo,
    JsonRpcConnection, Navigation, Notifications, Permissions, PlatformInfo, PreimageHost,
    ProductStorage, RuntimeConfig, RuntimeConfigValidationError, SessionUiInfo, ThemeHost,
    UserConfirmation, UserConfirmationReview,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::subscription::Spawner;
use crate::{FrameSink, HostCore, PermissionAuthorizationRequest, PermissionAuthorizationStatus};

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
    core_storage_read: Function,
    core_storage_write: Function,
    core_storage_clear: Function,
    confirm_user_action: Option<Function>,
    submit_preimage: Option<Function>,
    lookup_preimage: Option<Function>,
    subscribe_theme: Option<Function>,
    auth_state_changed: Option<Function>,
    /// Optional. Hosts that own JSON-RPC connections provide this; otherwise
    /// chain calls fail with an "unavailable" reason.
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
            local_storage_read: get_function(callbacks, "read")?,
            local_storage_write: get_function(callbacks, "write")?,
            local_storage_clear: get_function(callbacks, "clear")?,
            core_storage_read: get_function(callbacks, "readCoreStorage")?,
            core_storage_write: get_function(callbacks, "writeCoreStorage")?,
            core_storage_clear: get_function(callbacks, "clearCoreStorage")?,
            confirm_user_action: get_optional_function(callbacks, "confirmUserAction")?,
            submit_preimage: get_optional_function(callbacks, "submitPreimage")?,
            lookup_preimage: get_optional_function(callbacks, "lookupPreimage")?,
            subscribe_theme: get_optional_function(callbacks, "subscribeTheme")?,
            auth_state_changed: get_optional_function(callbacks, "authStateChanged")?,
            chain_connect: get_optional_function(callbacks, "chainConnect")?,
            emit_frame: get_function(callbacks, "emitFrame")?,
            dispose: get_optional_function(callbacks, "dispose")?.unwrap_or_else(noop_function),
        })
    }
}

struct WasmFrameSink {
    bridge: SendWrapper<Arc<JsBridge>>,
}

impl FrameSink for WasmFrameSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        let frame = Uint8Array::from(frame.as_slice());
        if let Err(err) = self.bridge.emit_frame.call1(&JsValue::NULL, &frame) {
            web_sys::console::error_1(&err);
        }
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

#[truapi_platform::async_trait]
impl Navigation for WasmPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        invoke_navigate_to(&self.bridge, &url)
            .await
            .map_err(|reason| v01::HostNavigateToError::Unknown { reason })
    }
}

#[truapi_platform::async_trait]
impl Notifications for WasmPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        let bytes = invoke_bytes_return(&self.bridge.push_notification, notification.encode())
            .await
            .map_err(generic)?;
        v01::HostPushNotificationResponse::decode(&mut bytes.as_slice())
            .map_err(|_| generic("pushNotification response did not decode".to_string()))
    }

    async fn cancel_notification(&self, id: v01::NotificationId) -> Result<(), v01::GenericError> {
        let Some(fn_) = self.bridge.cancel_notification.as_ref() else {
            return Ok(());
        };
        invoke_u32_unit(fn_, id).await.map_err(generic)
    }
}

#[truapi_platform::async_trait]
impl Permissions for WasmPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        let bytes = invoke_bytes_return(&self.bridge.device_permission, request.encode())
            .await
            .map_err(generic)?;
        v01::HostDevicePermissionResponse::decode(&mut bytes.as_slice())
            .map_err(|_| generic("devicePermission response did not decode".to_string()))
    }

    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        let bytes = invoke_bytes_return(&self.bridge.remote_permission, request.encode())
            .await
            .map_err(generic)?;
        v01::RemotePermissionResponse::decode(&mut bytes.as_slice())
            .map_err(|_| generic("remotePermission response did not decode".to_string()))
    }
}

#[truapi_platform::async_trait]
impl Features for WasmPlatform {
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        let bytes = invoke_bytes_return(&self.bridge.feature_supported, request.encode())
            .await
            .map_err(generic)?;
        v01::HostFeatureSupportedResponse::decode(&mut bytes.as_slice())
            .map_err(|_| generic("featureSupported response did not decode".to_string()))
    }
}

#[truapi_platform::async_trait]
impl ProductStorage for WasmPlatform {
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

#[truapi_platform::async_trait]
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
                closed: AtomicBool::new(false),
                _on_response: SendWrapper::new(on_response),
                response_rx: std::sync::Mutex::new(Some(response_rx)),
            }) as Box<dyn JsonRpcConnection>)
        })
        .await
    }
}

impl AuthPresenter for WasmPlatform {
    fn auth_state_changed(&self, state: AuthState) {
        let Some(fn_) = self.bridge.auth_state_changed.as_ref() else {
            return;
        };
        if let Err(err) = fn_.call1(&JsValue::NULL, &auth_state_to_js(&state)) {
            web_sys::console::error_1(&err);
        }
    }
}

#[truapi_platform::async_trait]
impl CoreStorage for WasmPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        invoke_core_storage_read(&self.bridge, key)
            .await
            .map_err(generic)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        invoke_core_storage_write(&self.bridge, key, value)
            .await
            .map_err(generic)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        invoke_core_storage_clear(&self.bridge, key)
            .await
            .map_err(generic)
    }
}

#[truapi_platform::async_trait]
impl UserConfirmation for WasmPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        let Some(fn_) = self.bridge.confirm_user_action.as_ref() else {
            return Ok(false);
        };
        invoke_bool(fn_, review.encode()).await.map_err(generic)
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

#[truapi_platform::async_trait]
impl PreimageHost for WasmPlatform {
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
    closed: AtomicBool,
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

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.close_fn.call0(&JsValue::NULL);
    }
}

impl Drop for JsCallbackJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
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
) -> impl Future<Output = Result<(), String>> + Send {
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
) -> impl Future<Output = Result<bool, String>> + Send {
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

fn invoke_u32_unit(fn_: &Function, value: u32) -> impl Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let arg = JsValue::from_f64(f64::from(value));
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_bytes_return(
    fn_: &Function,
    value: Vec<u8>,
) -> impl Future<Output = Result<Vec<u8>, String>> + Send {
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

/// Plain JS object mirroring the generated `AuthState` TS tagged union:
/// `{ tag, value }` with `value` omitted for unit variants.
fn auth_state_to_js(state: &AuthState) -> JsValue {
    let object = js_sys::Object::new();
    let set = |key: &str, value: &JsValue| {
        let _ = Reflect::set(&object, &JsValue::from_str(key), value);
    };
    match state {
        AuthState::Disconnected => {
            set("tag", &JsValue::from_str("Disconnected"));
        }
        AuthState::Pairing { deeplink } => {
            set("tag", &JsValue::from_str("Pairing"));
            let value = js_sys::Object::new();
            let _ = Reflect::set(
                &value,
                &JsValue::from_str("deeplink"),
                &JsValue::from_str(deeplink),
            );
            set("value", &value.into());
        }
        AuthState::Connected(info) => {
            set("tag", &JsValue::from_str("Connected"));
            set("value", &session_ui_info_to_js(info));
        }
        AuthState::LoginFailed { reason } => {
            set("tag", &JsValue::from_str("LoginFailed"));
            let value = js_sys::Object::new();
            let _ = Reflect::set(
                &value,
                &JsValue::from_str("reason"),
                &JsValue::from_str(reason),
            );
            set("value", &value.into());
        }
    }
    object.into()
}

/// Plain JS object mirroring the generated `SessionUiInfo` TS interface.
fn session_ui_info_to_js(info: &SessionUiInfo) -> JsValue {
    let object = js_sys::Object::new();
    let set = |key: &str, value: &JsValue| {
        let _ = Reflect::set(&object, &JsValue::from_str(key), value);
    };
    set("publicKey", &Uint8Array::from(info.public_key.as_slice()));
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

fn invoke_core_storage_read(
    bridge: &JsBridge,
    key: CoreStorageKey,
) -> impl Future<Output = Result<Option<Vec<u8>>, String>> + Send {
    let fn_ = bridge.core_storage_read.clone();
    SendWrapper::new(async move {
        let key_arg = Uint8Array::from(key.encode().as_slice());
        let returned = fn_.call1(&JsValue::NULL, &key_arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        if resolved.is_null() || resolved.is_undefined() {
            return Ok(None);
        }
        let array = resolved.dyn_into::<Uint8Array>().map_err(|_| {
            "readCoreStorage must resolve to Uint8Array, null or undefined".to_string()
        })?;
        Ok(Some(array.to_vec()))
    })
}

fn invoke_core_storage_write(
    bridge: &JsBridge,
    key: CoreStorageKey,
    value: Vec<u8>,
) -> impl Future<Output = Result<(), String>> + Send {
    let fn_ = bridge.core_storage_write.clone();
    SendWrapper::new(async move {
        let key_arg = Uint8Array::from(key.encode().as_slice());
        let value_arg = Uint8Array::from(value.as_slice());
        let returned = fn_
            .call2(&JsValue::NULL, &key_arg, &value_arg)
            .map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_core_storage_clear(
    bridge: &JsBridge,
    key: CoreStorageKey,
) -> impl Future<Output = Result<(), String>> + Send {
    let fn_ = bridge.core_storage_clear.clone();
    SendWrapper::new(async move {
        let key_arg = Uint8Array::from(key.encode().as_slice());
        let returned = fn_.call1(&JsValue::NULL, &key_arg).map_err(js_to_string)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_local_storage_read(
    bridge: &JsBridge,
    key: &str,
) -> impl Future<Output = Result<Option<Vec<u8>>, String>> + Send {
    let fn_ = bridge.local_storage_read.clone();
    let key = key.to_string();
    SendWrapper::new(async move {
        let arg = JsValue::from_str(&key);
        let returned = fn_.call1(&JsValue::NULL, &arg).map_err(js_to_string)?;
        let resolved = await_optional_promise(returned).await?;
        if resolved.is_null() || resolved.is_undefined() {
            return Ok(None);
        }
        let array = resolved
            .dyn_into::<Uint8Array>()
            .map_err(|_| "read must resolve to Uint8Array, null or undefined".to_string())?;
        Ok(Some(array.to_vec()))
    })
}

fn invoke_local_storage_write(
    bridge: &JsBridge,
    key: &str,
    value: &[u8],
) -> impl Future<Output = Result<(), String>> + Send {
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
) -> impl Future<Output = Result<(), String>> + Send {
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

    let host = get_required_object(value, "host", "runtimeConfig.host")?;
    let platform = get_optional_object(value, "platform", "runtimeConfig.platform")?;
    let people = get_required_object(value, "people", "runtimeConfig.people")?;
    let pairing = get_required_object(value, "pairing", "runtimeConfig.pairing")?;

    RuntimeConfig::new(
        get_required_string_at(value, "productId", "runtimeConfig.productId")?,
        HostInfo {
            name: get_required_string_at(&host, "name", "runtimeConfig.host.name")?,
            icon: get_optional_string_at(&host, "icon", "runtimeConfig.host.icon")?,
            version: get_optional_string_at(&host, "version", "runtimeConfig.host.version")?,
        },
        PlatformInfo {
            kind: platform
                .as_ref()
                .map(|p| get_optional_string_at(p, "type", "runtimeConfig.platform.type"))
                .transpose()?
                .flatten(),
            version: platform
                .as_ref()
                .map(|p| get_optional_string_at(p, "version", "runtimeConfig.platform.version"))
                .transpose()?
                .flatten(),
        },
        get_required_bytes32_at(&people, "genesisHash", "runtimeConfig.people.genesisHash")?,
        get_required_string_at(
            &pairing,
            "deeplinkScheme",
            "runtimeConfig.pairing.deeplinkScheme",
        )?,
    )
    .map_err(runtime_config_validation_to_js)
}

fn runtime_config_field_to_js(field: &str) -> &str {
    match field {
        "product_id" => "productId",
        "host_info.name" => "host.name",
        "pairing_deeplink_scheme" => "pairing.deeplinkScheme",
        "people_chain_genesis_hash" => "people.genesisHash",
        other => other,
    }
}

fn runtime_config_validation_to_js(err: RuntimeConfigValidationError) -> JsValue {
    match err {
        RuntimeConfigValidationError::EmptyField { field } => JsValue::from_str(&format!(
            "runtimeConfig.{} must not be empty",
            runtime_config_field_to_js(field)
        )),
        RuntimeConfigValidationError::InvalidHostIcon { reason } => JsValue::from_str(&format!(
            "runtimeConfig.host.icon must be an absolute HTTPS URL: {reason}"
        )),
        RuntimeConfigValidationError::InsecureHostIcon { scheme } => JsValue::from_str(&format!(
            "runtimeConfig.host.icon must use https scheme, got {scheme:?}"
        )),
        RuntimeConfigValidationError::InvalidDeeplinkScheme { scheme } => JsValue::from_str(
            &format!("runtimeConfig.pairing.deeplinkScheme must not include ://, got {scheme:?}"),
        ),
    }
}

fn get_required_object(value: &JsValue, name: &str, path: &str) -> Result<JsValue, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Err(JsValue::from_str(&format!("{path} is required")));
    }
    if !property.is_object() {
        return Err(JsValue::from_str(&format!("{path} must be an object")));
    }
    Ok(property)
}

fn get_optional_object(
    value: &JsValue,
    name: &str,
    path: &str,
) -> Result<Option<JsValue>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    if !property.is_object() {
        return Err(JsValue::from_str(&format!("{path} must be an object")));
    }
    Ok(Some(property))
}

fn get_optional_string_at(
    value: &JsValue,
    name: &str,
    path: &str,
) -> Result<Option<String>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    property
        .as_string()
        .map(Some)
        .ok_or_else(|| JsValue::from_str(&format!("{path} must be a string")))
}

fn get_required_string_at(value: &JsValue, name: &str, path: &str) -> Result<String, JsValue> {
    get_optional_string_at(value, name, path)?
        .ok_or_else(|| JsValue::from_str(&format!("{path} is required")))
}

fn get_optional_bytes32_at(
    value: &JsValue,
    name: &str,
    path: &str,
) -> Result<Option<[u8; 32]>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    if let Some(hex) = property.as_string() {
        return parse_hex32(&hex)
            .map(Some)
            .map_err(|reason| JsValue::from_str(&format!("{path}: {reason}")));
    }
    let array = property
        .dyn_into::<Uint8Array>()
        .map_err(|_| JsValue::from_str(&format!("{path} must be hex or Uint8Array")))?;
    let bytes = array.to_vec();
    bytes.try_into().map(Some).map_err(|bytes: Vec<u8>| {
        JsValue::from_str(&format!(
            "{path} must be exactly 32 bytes, got {}",
            bytes.len()
        ))
    })
}

fn get_required_bytes32_at(value: &JsValue, name: &str, path: &str) -> Result<[u8; 32], JsValue> {
    get_optional_bytes32_at(value, name, path)?
        .ok_or_else(|| JsValue::from_str(&format!("{path} is required")))
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

fn decode_permission_authorization_request(
    payload: &[u8],
) -> Result<PermissionAuthorizationRequest, JsValue> {
    PermissionAuthorizationRequest::decode(&mut &*payload).map_err(|err| {
        JsValue::from_str(&format!(
            "permission authorization request did not decode: {err}"
        ))
    })
}

fn decode_permission_authorization_requests(
    payloads: &Array,
) -> Result<Vec<PermissionAuthorizationRequest>, JsValue> {
    let mut requests = Vec::with_capacity(payloads.length() as usize);
    for payload in payloads.iter() {
        let payload = payload
            .dyn_into::<Uint8Array>()
            .map_err(|_| JsValue::from_str("permission authorization request must be bytes"))?;
        requests.push(decode_permission_authorization_request(&payload.to_vec())?);
    }
    Ok(requests)
}

fn permission_authorization_status_to_js(status: PermissionAuthorizationStatus) -> JsValue {
    JsValue::from_str(match status {
        PermissionAuthorizationStatus::NotDetermined => "NotDetermined",
        PermissionAuthorizationStatus::Denied => "Denied",
        PermissionAuthorizationStatus::Authorized => "Authorized",
    })
}

fn permission_authorization_status_from_js(
    status: &str,
) -> Result<PermissionAuthorizationStatus, JsValue> {
    match status {
        "NotDetermined" => Ok(PermissionAuthorizationStatus::NotDetermined),
        "Denied" => Ok(PermissionAuthorizationStatus::Denied),
        "Authorized" => Ok(PermissionAuthorizationStatus::Authorized),
        other => Err(JsValue::from_str(&format!(
            "unknown permission authorization status: {other}"
        ))),
    }
}

fn generic_error_to_js(err: v01::GenericError) -> JsValue {
    JsValue::from_str(&err.reason)
}

struct WasmCoreInner {
    core: HostCore,
    dispose_fn: SendWrapper<Function>,
    disposed: Cell<bool>,
    disposing: Cell<bool>,
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`).
/// Hosts may call this during boot, or again at any time to re-tune verbosity.
/// Unknown values are parsed as `off`.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn set_log_level(level: &str) {
    crate::logging::set_level_from_str(level);
}

/// JS-callable handle to the TrUAPI core. Constructed once per shell boot.
#[wasm_bindgen]
pub struct WasmHostCore {
    inner: Rc<WasmCoreInner>,
}

#[wasm_bindgen]
impl WasmHostCore {
    /// Build the core from a JS callbacks object. The object must define
    /// every host capability the [`truapi_platform::Platform`] trait set
    /// requires (camelCase property names; see the source for the full
    /// list).
    #[wasm_bindgen(constructor)]
    pub fn new(callbacks: JsValue, runtime_config: JsValue) -> Result<WasmHostCore, JsValue> {
        // Surface Rust panics to the browser console. A panic mid-dispatch
        // aborts the call as a wasm trap; the host should treat a thrown error
        // from `receiveFrame` as a fatal-instance signal and rebuild the
        // core rather than continue using it.
        console_error_panic_hook::set_once();
        crate::logging::init();
        let bridge = Arc::new(JsBridge::from_js(&callbacks)?);
        let frame_sink = Arc::new(WasmFrameSink {
            bridge: SendWrapper::new(bridge.clone()),
        });
        let dispose_fn = SendWrapper::new(bridge.dispose.clone());
        let platform = Arc::new(WasmPlatform::new(bridge));
        let spawner: Spawner = Arc::new(|fut| {
            wasm_bindgen_futures::spawn_local(fut);
        });
        let runtime_config = runtime_config_from_js(&runtime_config)?;
        let core =
            HostCore::from_platform_with_config(platform, runtime_config, spawner, frame_sink);
        Ok(Self {
            inner: Rc::new(WasmCoreInner {
                core,
                dispose_fn,
                disposed: Cell::new(false),
                disposing: Cell::new(false),
            }),
        })
    }

    /// Push a SCALE-encoded protocol frame into the dispatcher. Responses
    /// (and subscription items) flow back through the `emitFrame`
    /// callback.
    #[wasm_bindgen(js_name = receiveFrame)]
    pub async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), JsValue> {
        self.inner
            .core
            .receive_frame(frame)
            .await
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Read a stored permission authorization status without prompting.
    ///
    /// `payload` is a SCALE-encoded `PermissionAuthorizationRequest`.
    #[wasm_bindgen(js_name = permissionAuthorizationStatus)]
    pub async fn permission_authorization_status(
        &self,
        payload: Vec<u8>,
    ) -> Result<JsValue, JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = self
            .inner
            .core
            .permission_authorization_status(request)
            .await
            .map_err(generic_error_to_js)?;
        Ok(permission_authorization_status_to_js(status))
    }

    /// Read stored permission authorization statuses without prompting.
    ///
    /// `payloads` is an array of SCALE-encoded
    /// `PermissionAuthorizationRequest` values. Results follow the same order.
    #[wasm_bindgen(js_name = permissionAuthorizationStatuses)]
    pub async fn permission_authorization_statuses(
        &self,
        payloads: Array,
    ) -> Result<Array, JsValue> {
        let requests = decode_permission_authorization_requests(&payloads)?;
        let statuses = self
            .inner
            .core
            .permission_authorization_statuses(requests)
            .await
            .map_err(generic_error_to_js)?;
        let values = Array::new();
        for status in statuses {
            values.push(&permission_authorization_status_to_js(status));
        }
        Ok(values)
    }

    /// Update a stored permission authorization status. Passing
    /// `"NotDetermined"` clears the stored value so the next product request
    /// prompts again.
    #[wasm_bindgen(js_name = setPermissionAuthorizationStatus)]
    pub async fn set_permission_authorization_status(
        &self,
        payload: Vec<u8>,
        status: String,
    ) -> Result<(), JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = permission_authorization_status_from_js(&status)?;
        self.inner
            .core
            .set_permission_authorization_status(request, status)
            .await
            .map_err(generic_error_to_js)
    }

    /// Tear down the bridge. Invokes the JS-side `dispose` callback so the
    /// host can drop its end of the wiring.
    pub fn dispose(&self) -> Result<(), JsValue> {
        if self.inner.disposed.get() {
            return Ok(());
        }
        if self.inner.disposing.replace(true) {
            return Ok(());
        }

        self.inner.core.dispose();

        let result = self.inner.dispose_fn.call0(&JsValue::NULL).map(|_| ());

        self.inner.disposed.set(true);
        self.inner.disposing.set(false);
        result
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    #[wasm_bindgen(js_name = disconnectSession)]
    pub async fn disconnect_session(&self) -> Result<(), JsValue> {
        self.inner.core.disconnect_session().await;
        Ok(())
    }

    /// Cancel any in-flight `request_login` pairing. The host receives a
    /// `Disconnected` auth state immediately and the pending login resolves
    /// to `Rejected`. A no-op when no login is in progress.
    #[wasm_bindgen(js_name = cancelPairing)]
    pub fn cancel_pairing(&self) {
        self.inner.core.cancel_pairing();
    }

    /// Notify the core that the host-global auth session slot may have changed.
    #[wasm_bindgen(js_name = notifySessionStoreChanged)]
    pub fn notify_session_store_changed(&self) {
        if self.inner.disposed.get() {
            return;
        }
        self.inner.core.notify_session_store_changed();
    }
}
