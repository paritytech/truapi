//! Subscription lifecycle management.
//!
//! Tracks active subscriptions (start/receive/stop/interrupt) and handles
//! cleanup when either side terminates.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::future::{Either, select};
use futures::stream::BoxStream;
use parity_scale_codec::Encode;

use crate::frame::{FrameKind, Payload, ProtocolMessage, compose_action};
use crate::transport::Transport;

type StopFn = Box<dyn FnOnce() + Send>;

fn spawn_subscription<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    std::thread::spawn(move || {
        futures::executor::block_on(future);
    });
}

/// One yielded value of a subscription stream after SCALE-encoding.
pub enum SubscriptionOutput {
    /// A regular subscription item to deliver as a `_receive` frame.
    Item(Vec<u8>),
    /// Stream-initiated termination delivered as an `_interrupt` frame.
    Interrupt(Vec<u8>),
}

/// Boxed stream of [`SubscriptionOutput`] consumed by the dispatcher.
pub type SubscriptionStream = BoxStream<'static, SubscriptionOutput>;

/// Wrap a host-side stream of typed items into the SCALE-encoded
/// [`SubscriptionStream`] that the dispatcher delivers to the transport.
///
/// `Item` is the versioned wrapper for each emitted value (e.g.
/// `versioned::account::HostAccountConnectionStatusSubscribeItem`). The
/// generated dispatcher calls this with the second type parameter inferred
/// from the host trait return.
pub fn subscription_stream<Item, S>(stream: S) -> SubscriptionStream
where
    Item: Encode + 'static,
    S: futures::Stream<Item = Item> + Send + 'static,
{
    Box::pin(stream.map(|item| SubscriptionOutput::Item(item.encode())))
}

/// Manages active subscriptions on the server side.
pub struct SubscriptionManager {
    active: Arc<Mutex<HashMap<String, StopFn>>>,
}

impl SubscriptionManager {
    /// Create an empty manager.
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a subscription: forward stream items as `_receive` frames.
    /// Returns when the stream ends or `_stop` is received.
    pub fn register(
        &self,
        request_id: String,
        method: &str,
        mut stream: SubscriptionStream,
        transport: Arc<dyn Transport>,
    ) {
        let action = compose_action(method, FrameKind::Receive);
        let interrupt_action = compose_action(method, FrameKind::Interrupt);
        let completed_interrupt_action = interrupt_action.clone();
        let rid = request_id.clone();
        let stream_transport = transport.clone();

        // Cancellation channel.
        let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<()>();

        // Store the cancel handle.
        {
            let mut active = self.active.lock().unwrap();
            active.insert(
                request_id.clone(),
                Box::new(move || {
                    let _ = cancel_tx.send(());
                }),
            );
        }

        let active = self.active.clone();

        spawn_subscription(async move {
            let completed = {
                let mut cancel_rx = cancel_rx;
                loop {
                    match select(cancel_rx, stream.next()).await {
                        Either::Left((_cancelled, _next)) => break false,
                        Either::Right((item, next_cancel_rx)) => {
                            cancel_rx = next_cancel_rx;
                            match item {
                                Some(SubscriptionOutput::Item(value)) => {
                                    stream_transport.send(ProtocolMessage {
                                        request_id: rid.clone(),
                                        payload: Payload {
                                            tag: action.clone(),
                                            value,
                                        },
                                    })
                                }
                                Some(SubscriptionOutput::Interrupt(value)) => {
                                    stream_transport.send(ProtocolMessage {
                                        request_id: rid.clone(),
                                        payload: Payload {
                                            tag: interrupt_action.clone(),
                                            value,
                                        },
                                    });
                                    break false;
                                }
                                None => break true,
                            }
                        }
                    }
                }
            };

            let removed = {
                let mut active = active.lock().unwrap();
                active.remove(&request_id).is_some()
            };

            if completed && removed {
                transport.send(ProtocolMessage {
                    request_id,
                    payload: Payload {
                        tag: completed_interrupt_action,
                        value: Vec::new(),
                    },
                });
            }
        });
    }

    /// Handle a `_stop` frame from the product side.
    pub fn handle_stop(&self, request_id: &str) {
        let mut active = self.active.lock().unwrap();
        if let Some(cancel) = active.remove(request_id) {
            cancel();
        }
    }

    /// Send an `_interrupt` frame to the product side.
    pub fn interrupt(&self, request_id: &str, method: &str, transport: &dyn Transport) {
        let mut active = self.active.lock().unwrap();
        active.remove(request_id);
        let msg = ProtocolMessage {
            request_id: request_id.to_string(),
            payload: Payload {
                tag: compose_action(method, FrameKind::Interrupt),
                value: Vec::new(),
            },
        };
        transport.send(msg);
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    /// Transport that records every frame and notifies waiters when it
    /// reaches a target count. Used to wait for the subscription's
    /// background thread to drain a known number of frames.
    struct RecordingTransport {
        sent: Mutex<Vec<ProtocolMessage>>,
        cvar: std::sync::Condvar,
    }

    impl RecordingTransport {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                cvar: std::sync::Condvar::new(),
            }
        }
        fn sent(&self) -> Vec<ProtocolMessage> {
            self.sent.lock().unwrap().clone()
        }
        /// Wait until at least `count` frames have been recorded, or
        /// `timeout` elapses. Returns the number of frames recorded at
        /// wake-up time.
        fn wait_for(&self, count: usize, timeout: std::time::Duration) -> usize {
            let mut guard = self.sent.lock().unwrap();
            let deadline = std::time::Instant::now() + timeout;
            while guard.len() < count {
                let now = std::time::Instant::now();
                if now >= deadline {
                    break;
                }
                let (new_guard, _) = self.cvar.wait_timeout(guard, deadline - now).unwrap();
                guard = new_guard;
            }
            guard.len()
        }
    }

    impl Transport for RecordingTransport {
        fn send(&self, message: ProtocolMessage) {
            self.sent.lock().unwrap().push(message);
            self.cvar.notify_all();
        }
        fn on_message(
            &self,
            _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
        ) -> Box<dyn FnOnce()> {
            Box::new(|| {})
        }
    }

    fn dummy_stream(items: Vec<Vec<u8>>) -> SubscriptionStream {
        Box::pin(stream::iter(
            items.into_iter().map(SubscriptionOutput::Item),
        ))
    }

    /// Register a never-ending stream then immediately stop it. The
    /// stream's first poll must observe cancellation and exit without
    /// having pushed any frame.
    #[test]
    fn register_then_stop_emits_no_extra_frames() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new();
        let slow_stream: SubscriptionStream = Box::pin(stream::pending());
        manager.register("p:1".to_string(), "demo_method", slow_stream, transport_dyn);
        manager.handle_stop("p:1");
        // Give the worker thread a beat to observe the cancel.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            transport_typed.sent().is_empty(),
            "stopped subscription must not push any frame"
        );
    }

    /// A stream that yields 2 items then ends naturally must produce 2
    /// `_receive` frames followed by one `_interrupt` frame.
    #[test]
    fn register_completion_emits_interrupt() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new();
        let items = dummy_stream(vec![vec![0xaa], vec![0xbb]]);
        manager.register("p:1".to_string(), "demo_method", items, transport_dyn);
        let observed = transport_typed.wait_for(3, std::time::Duration::from_secs(2));
        assert_eq!(observed, 3, "expected 2 receive frames + 1 interrupt");
        let frames = transport_typed.sent();
        assert_eq!(frames[0].payload.tag, "demo_method_receive");
        assert_eq!(frames[0].payload.value, vec![0xaa]);
        assert_eq!(frames[1].payload.tag, "demo_method_receive");
        assert_eq!(frames[1].payload.value, vec![0xbb]);
        assert_eq!(frames[2].payload.tag, "demo_method_interrupt");
        assert_eq!(frames[2].payload.value, Vec::<u8>::new());
    }

    /// Calling `handle_stop` twice on the same request id must be a
    /// no-op the second time around (the entry has already been removed,
    /// no panic, no extra frames).
    #[test]
    fn double_stop_is_idempotent() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new();
        let slow_stream: SubscriptionStream = Box::pin(stream::pending());
        manager.register("p:1".to_string(), "demo_method", slow_stream, transport_dyn);
        manager.handle_stop("p:1");
        // Second call must not panic and must not emit any frame.
        manager.handle_stop("p:1");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            transport_typed.sent().is_empty(),
            "double-stop must not emit any frame"
        );
    }
}
