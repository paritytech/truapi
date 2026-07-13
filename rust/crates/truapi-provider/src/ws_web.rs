//! Remote WebSocket JSON-RPC backend for `wasm32` (browser) targets.
//!
//! Mirrors the native backend's contract over the browser's `WebSocket`: a
//! raw string pipe whose responses stream ends when the socket dies. wasm is
//! single-threaded, so `SendWrapper` satisfies the trait's `Send + Sync`
//! bounds without real cross-thread use — calling a connection from another
//! thread would panic, and no such thread exists in this environment.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::{mpsc, oneshot};
use futures::stream::{BoxStream, StreamExt};
use send_wrapper::SendWrapper;
use truapi::latest::GenericError;
use truapi_platform::JsonRpcConnection;
use url::Url;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

/// The JS event callbacks kept alive for the socket's lifetime
/// (onmessage, onopen, onerror, onclose).
type SocketCallbacks = (
    Closure<dyn FnMut(MessageEvent)>,
    Closure<dyn FnMut(Event)>,
    Closure<dyn FnMut(Event)>,
    Closure<dyn FnMut(CloseEvent)>,
);

/// The socket and its live JS callbacks, wrapped so the connect future (which
/// the trait requires to be `Send`) can hold them across the handshake await.
type StagedSocket = SendWrapper<(WebSocket, SocketCallbacks)>;

/// Open a browser WebSocket connection to `url`.
pub(crate) async fn connect(url: Url) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
    let (staged, handshake_rx, responses_tx, responses_rx) = open_socket(&url)?;

    match handshake_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => {
            return Err(GenericError {
                reason: format!("WebSocket handshake with {url} failed: {reason}"),
            });
        }
        Err(_) => {
            return Err(GenericError {
                reason: format!("WebSocket handshake with {url} was abandoned"),
            });
        }
    }

    let (socket, callbacks) = staged.take();
    Ok(Box::new(WebWsConnection {
        socket: SendWrapper::new(socket),
        _callbacks: SendWrapper::new(callbacks),
        responses_close: responses_tx,
        responses: Mutex::new(Some(responses_rx.boxed())),
        closed: AtomicBool::new(false),
    }))
}

/// Create the socket and wire its callbacks.
///
/// Synchronous on purpose: every non-`Send` JS value is created and wrapped
/// here so the awaiting caller only holds `Send` state.
#[allow(clippy::type_complexity)]
fn open_socket(
    url: &Url,
) -> Result<
    (
        StagedSocket,
        oneshot::Receiver<Result<(), String>>,
        mpsc::UnboundedSender<String>,
        mpsc::UnboundedReceiver<String>,
    ),
    GenericError,
> {
    let socket = WebSocket::new(url.as_str()).map_err(|err| GenericError {
        reason: format!("WebSocket creation for {url} failed: {err:?}"),
    })?;
    socket.set_binary_type(BinaryType::Arraybuffer);

    let (responses_tx, responses_rx) = mpsc::unbounded::<String>();
    // Resolved exactly once by whichever of onopen/onerror/onclose fires
    // first, so the handshake can be awaited.
    let handshake = Rc::new(RefCell::new(None::<oneshot::Sender<Result<(), String>>>));
    let (handshake_tx, handshake_rx) = oneshot::channel();
    *handshake.borrow_mut() = Some(handshake_tx);

    let onmessage = {
        let responses_tx = responses_tx.clone();
        Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let _ = responses_tx.unbounded_send(text);
            } else if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                match String::from_utf8(bytes) {
                    Ok(text) => {
                        let _ = responses_tx.unbounded_send(text);
                    }
                    Err(_) => tracing::warn!("dropping non-UTF-8 binary WebSocket frame"),
                }
            }
        })
    };
    let onopen = {
        let handshake = Rc::clone(&handshake);
        Closure::<dyn FnMut(Event)>::new(move |_| {
            if let Some(sender) = handshake.borrow_mut().take() {
                let _ = sender.send(Ok(()));
            }
        })
    };
    let onerror = {
        let handshake = Rc::clone(&handshake);
        Closure::<dyn FnMut(Event)>::new(move |_| {
            if let Some(sender) = handshake.borrow_mut().take() {
                let _ = sender.send(Err("WebSocket error during handshake".to_owned()));
            }
        })
    };
    let onclose = {
        let handshake = Rc::clone(&handshake);
        let responses_tx = responses_tx.clone();
        Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
            if let Some(sender) = handshake.borrow_mut().take() {
                let _ = sender.send(Err(format!(
                    "WebSocket closed during handshake (code {})",
                    event.code()
                )));
            }
            // Ending the channel ends the responses stream — the disconnect
            // signal consumers rely on.
            responses_tx.close_channel();
        })
    };
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

    Ok((
        SendWrapper::new((socket, (onmessage, onopen, onerror, onclose))),
        handshake_rx,
        responses_tx,
        responses_rx,
    ))
}

/// A live browser WebSocket connection exposed as a raw JSON-RPC pipe.
struct WebWsConnection {
    socket: SendWrapper<WebSocket>,
    /// Keeps the JS event callbacks alive for the socket's lifetime.
    _callbacks: SendWrapper<SocketCallbacks>,
    /// Sender half of the responses channel, kept to end the stream on close.
    responses_close: mpsc::UnboundedSender<String>,
    responses: Mutex<Option<BoxStream<'static, String>>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for WebWsConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(err) = self.socket.send_with_str(&request) {
            // A send failure on a browser WebSocket means the socket is dead.
            // End the responses stream so the consumer — which correlates by
            // id — sees a disconnect instead of hanging on a request that will
            // never be answered.
            tracing::warn!("WebSocket send failed: {err:?}");
            self.closed.store(true, Ordering::SeqCst);
            self.responses_close.close_channel();
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        match self
            .responses
            .lock()
            .expect("responses mutex poisoned")
            .take()
        {
            Some(responses) => responses,
            None => futures::stream::empty().boxed(),
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.socket.close();
        self.responses_close.close_channel();
        self.responses
            .lock()
            .expect("responses mutex poisoned")
            .take();
    }
}

impl Drop for WebWsConnection {
    fn drop(&mut self) {
        self.close();
    }
}
