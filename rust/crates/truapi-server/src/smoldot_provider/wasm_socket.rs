// SPDX-License-Identifier: Apache-2.0
// Vendored from subxt-lightclient 0.50.0 (Apache-2.0 OR GPL-3.0), elected
// under Apache-2.0. See ../../THIRD_PARTY_NOTICES.md for attribution.

use futures::{io, prelude::*};
use send_wrapper::SendWrapper;
use wasm_bindgen::{JsCast, prelude::*};

use derive_more::Display;
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    task::{Context, Waker},
};

/// Errors returned by the wasm-side socket constructor.
#[derive(Debug, Display)]
pub enum Error {
    /// Wraps a JS-side connection failure.
    #[display("Failed to connect {_0}")]
    ConnectionError(String),
}

impl std::error::Error for Error {}

/// WebSocket for WASM environments. Not safe to clone because the inner
/// `web_sys::WebSocket` is not thread-safe and the wakers borrow the same
/// `Mutex`.
pub struct WasmSocket {
    inner: Arc<Mutex<InnerWasmSocket>>,
    socket: SendWrapper<web_sys::WebSocket>,
    _callbacks: SendWrapper<Callbacks>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ConnectionState {
    Connecting,
    Opened,
    Closed,
    Error,
}

struct InnerWasmSocket {
    state: ConnectionState,
    data: VecDeque<u8>,
    waker: Option<Waker>,
}

type Callbacks = (
    Closure<dyn FnMut()>,
    Closure<dyn FnMut(web_sys::MessageEvent)>,
    Closure<dyn FnMut(web_sys::Event)>,
    Closure<dyn FnMut(web_sys::CloseEvent)>,
);

impl WasmSocket {
    /// Opens a new WebSocket connection to the given URL. Returns an error
    /// if the browser refuses to construct the underlying `WebSocket`
    /// object.
    pub fn new(addr: &str) -> Result<Self, Error> {
        let socket = match web_sys::WebSocket::new(addr) {
            Ok(socket) => socket,
            Err(err) => return Err(Error::ConnectionError(format!("{err:?}"))),
        };

        socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let inner = Arc::new(Mutex::new(InnerWasmSocket {
            state: ConnectionState::Connecting,
            data: VecDeque::with_capacity(16384),
            waker: None,
        }));

        let open_callback = Closure::<dyn FnMut()>::new({
            let inner = inner.clone();
            move || {
                let mut inner = inner.lock().expect("Mutex is poised; qed");
                inner.state = ConnectionState::Opened;

                if let Some(waker) = inner.waker.take() {
                    waker.wake();
                }
            }
        });
        socket.set_onopen(Some(open_callback.as_ref().unchecked_ref()));

        let message_callback = Closure::<dyn FnMut(_)>::new({
            let inner = inner.clone();
            move |event: web_sys::MessageEvent| {
                let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else {
                    panic!("Unexpected data format {:?}", event.data());
                };

                let mut inner = inner.lock().expect("Mutex is poised; qed");
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                inner.data.extend(bytes);

                if let Some(waker) = inner.waker.take() {
                    waker.wake();
                }
            }
        });
        socket.set_onmessage(Some(message_callback.as_ref().unchecked_ref()));

        let error_callback = Closure::<dyn FnMut(_)>::new({
            let inner = inner.clone();
            move |_event: web_sys::Event| {
                let mut inner = inner.lock().expect("Mutex is poised; qed");
                inner.state = ConnectionState::Error;

                if let Some(waker) = inner.waker.take() {
                    waker.wake();
                }
            }
        });
        socket.set_onerror(Some(error_callback.as_ref().unchecked_ref()));

        let close_callback = Closure::<dyn FnMut(_)>::new({
            let inner = inner.clone();
            move |_event: web_sys::CloseEvent| {
                let mut inner = inner.lock().expect("Mutex is poised; qed");
                inner.state = ConnectionState::Closed;

                if let Some(waker) = inner.waker.take() {
                    waker.wake();
                }
            }
        });
        socket.set_onclose(Some(close_callback.as_ref().unchecked_ref()));

        let callbacks = (
            open_callback,
            message_callback,
            error_callback,
            close_callback,
        );

        Ok(Self {
            inner,
            socket: SendWrapper::new(socket),
            _callbacks: SendWrapper::new(callbacks),
        })
    }
}

impl AsyncRead for WasmSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = self.inner.lock().expect("Mutex is poised; qed");
        inner.waker = Some(cx.waker().clone());

        if self.socket.ready_state() == web_sys::WebSocket::CONNECTING {
            return Poll::Pending;
        }

        match inner.state {
            ConnectionState::Error => Poll::Ready(Err(io::Error::other("Socket error"))),
            ConnectionState::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            ConnectionState::Connecting => Poll::Pending,
            ConnectionState::Opened => {
                if inner.data.is_empty() {
                    return Poll::Pending;
                }

                let n = inner.data.len().min(buf.len());
                for k in buf.iter_mut().take(n) {
                    *k = inner.data.pop_front().expect("Buffer non empty; qed");
                }
                Poll::Ready(Ok(n))
            }
        }
    }
}

impl AsyncWrite for WasmSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut inner = self.inner.lock().expect("Mutex is poised; qed");
        inner.waker = Some(cx.waker().clone());

        match inner.state {
            ConnectionState::Error => Poll::Ready(Err(io::Error::other("Socket error"))),
            ConnectionState::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            ConnectionState::Connecting => Poll::Pending,
            ConnectionState::Opened => match self.socket.send_with_u8_array(buf) {
                Ok(()) => Poll::Ready(Ok(buf.len())),
                Err(err) => Poll::Ready(Err(io::Error::other(format!("Write error: {err:?}")))),
            },
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if self.socket.ready_state() == web_sys::WebSocket::CLOSED {
            return Poll::Ready(Ok(()));
        }

        if self.socket.ready_state() != web_sys::WebSocket::CLOSING {
            let _ = self.socket.close();
        }

        let mut inner = self.inner.lock().expect("Mutex is poised; qed");
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for WasmSocket {
    fn drop(&mut self) {
        if self.socket.ready_state() != web_sys::WebSocket::CLOSING {
            let _ = self.socket.close();
        }

        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onerror(None);
        self.socket.set_onclose(None);
    }
}
