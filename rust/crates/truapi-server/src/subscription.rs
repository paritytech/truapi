//! Subscription lifecycle management.
//!
//! Tracks active subscriptions (start/receive/stop/interrupt) and handles
//! cleanup when either side terminates. Each registered subscription drives
//! its stream on a caller-supplied [`Spawner`]; the manager itself never
//! creates threads or runtimes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::future::{BoxFuture, Either, select};
use futures::stream::BoxStream;
use parity_scale_codec::Encode;

use crate::frame::{FrameKind, Payload, ProtocolMessage, compose_action};
use crate::transport::Transport;

type StopFn = Box<dyn FnOnce() + Send>;

/// Spawns a subscription-driving future onto the caller's runtime. The
/// future is `Send` because the inner [`SubscriptionStream`] is a
/// `BoxStream<'static, _>` and every captured value the manager threads
/// through it is also `Send`. Each platform bridge supplies an
/// implementation that hands the future to the runtime driving its
/// transport (tokio `LocalSet`, `wasm_bindgen_futures::spawn_local`, ...).
pub type Spawner = Arc<dyn Fn(BoxFuture<'static, ()>) + Send + Sync>;

/// Convenience spawner for tests and embedders that don't yet wire a
/// real runtime: starts a fresh OS thread per subscription and drives the
/// future with `futures::executor::block_on`. Not available on wasm32 since
/// the platform has no threads.
#[cfg(not(target_arch = "wasm32"))]
pub fn thread_per_subscription_spawner() -> Spawner {
    Arc::new(|fut: BoxFuture<'static, ()>| {
        std::thread::spawn(move || futures::executor::block_on(fut));
    })
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

/// Generation-stamped slot tracking the lifecycle of one subscription id.
/// `request_id` is client-controlled and may be reused or raced against a
/// `_stop`, so each reservation carries a monotonic generation and only the
/// owner of the current generation may transition or remove the slot.
enum Slot {
    /// Reserved by the dispatcher before its `_start` handler resolved.
    /// `cancelled` flips to `true` if a `_stop` arrives in that window so
    /// activation aborts instead of leaking an unstoppable stream.
    Pending { generation: u64, cancelled: bool },
    /// A live subscription with its cancellation handle.
    Live { generation: u64, cancel: StopFn },
}

/// Handle returned by [`SubscriptionManager::reserve`] and presented back to
/// [`SubscriptionManager::activate`]. Ties an activation to the exact
/// reservation it belongs to so a superseding `_start` for the same id
/// cannot be activated by a stale handler.
pub struct ReservationToken {
    request_id: String,
    generation: u64,
}

/// Manages active subscriptions on the server side.
pub struct SubscriptionManager {
    active: Arc<Mutex<HashMap<String, Slot>>>,
    next_generation: Arc<AtomicU64>,
    spawner: Spawner,
}

impl SubscriptionManager {
    /// Create an empty manager driven by `spawner`.
    pub fn new(spawner: Spawner) -> Self {
        Self {
            active: Arc::new(Mutex::new(HashMap::new())),
            next_generation: Arc::new(AtomicU64::new(0)),
            spawner,
        }
    }

    /// Reserve the slot for `request_id` before its subscription stream is
    /// available. Any live subscription already under that id is stopped and
    /// replaced (re-subscribe semantics). A `_stop` arriving before
    /// [`activate`](Self::activate) flips the reservation to cancelled.
    pub fn reserve(&self, request_id: String) -> ReservationToken {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let mut active = self.active.lock().unwrap();
        if let Some(Slot::Live { cancel, .. }) = active.insert(
            request_id.clone(),
            Slot::Pending {
                generation,
                cancelled: false,
            },
        ) {
            cancel();
        }
        ReservationToken {
            request_id,
            generation,
        }
    }

    /// Drop a reservation whose `_start` handler failed before producing a
    /// stream. No-op if the slot was superseded by a newer reservation.
    pub fn cancel_reservation(&self, token: ReservationToken) {
        let mut active = self.active.lock().unwrap();
        let owned = matches!(
            active.get(&token.request_id),
            Some(Slot::Pending { generation, .. }) if *generation == token.generation
        );
        if owned {
            active.remove(&token.request_id);
        }
    }

    /// Activate a reserved subscription with its stream, forwarding stream
    /// items as `_receive` frames until the stream ends or `_stop` is
    /// received. No-ops without starting the stream if the reservation was
    /// cancelled by a `_stop` or superseded by a newer reservation for the
    /// same id.
    pub fn activate(
        &self,
        token: ReservationToken,
        method: &str,
        mut stream: SubscriptionStream,
        transport: Arc<dyn Transport>,
    ) {
        let ReservationToken {
            request_id,
            generation,
        } = token;
        let action = compose_action(method, FrameKind::Receive);
        let interrupt_action = compose_action(method, FrameKind::Interrupt);
        let completed_interrupt_action = interrupt_action.clone();
        let rid = request_id.clone();
        let stream_transport = transport.clone();

        // Cancellation channel.
        let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<()>();

        // Transition the reserved slot to live, unless a `_stop` cancelled it
        // or a newer reservation superseded it while the handler resolved.
        {
            let mut active = self.active.lock().unwrap();
            match active.get(&request_id) {
                Some(Slot::Pending {
                    generation: g,
                    cancelled,
                }) if *g == generation => {
                    if *cancelled {
                        active.remove(&request_id);
                        return;
                    }
                }
                _ => return,
            }
            active.insert(
                request_id.clone(),
                Slot::Live {
                    generation,
                    cancel: Box::new(move || {
                        let _ = cancel_tx.send(());
                    }),
                },
            );
        }

        let active = self.active.clone();

        let future: BoxFuture<'static, ()> = Box::pin(async move {
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

            // Only remove the slot if it still holds THIS generation; a
            // superseding reservation owns its own cleanup.
            let removed = {
                let mut active = active.lock().unwrap();
                let owned = matches!(
                    active.get(&request_id),
                    Some(Slot::Live { generation: g, .. }) if *g == generation
                );
                if owned {
                    active.remove(&request_id);
                }
                owned
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

        (self.spawner)(future);
    }

    /// Convenience for callers that already hold the stream with no async gap
    /// between reservation and activation (tests and synchronous embedders).
    pub fn register(
        &self,
        request_id: String,
        method: &str,
        stream: SubscriptionStream,
        transport: Arc<dyn Transport>,
    ) {
        let token = self.reserve(request_id);
        self.activate(token, method, stream, transport);
    }

    /// Handle a `_stop` frame from the product side. Cancels a live
    /// subscription, or marks a still-pending reservation cancelled so its
    /// in-flight activation aborts rather than leaking an unstoppable stream.
    pub fn handle_stop(&self, request_id: &str) {
        let mut active = self.active.lock().unwrap();
        match active.get_mut(request_id) {
            Some(Slot::Pending { cancelled, .. }) => {
                *cancelled = true;
            }
            Some(Slot::Live { .. }) => {
                if let Some(Slot::Live { cancel, .. }) = active.remove(request_id) {
                    cancel();
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
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
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
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
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
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

    /// The manager must drive subscriptions through the injected spawner,
    /// not by reaching out to `std::thread::spawn` itself. The counter
    /// inside the test spawner is the proof.
    #[test]
    fn subscription_uses_provided_spawner_not_native_thread() {
        let invocations = Arc::new(AtomicUsize::new(0));
        let invocations_for_spawner = invocations.clone();
        let spawner: Spawner = Arc::new(move |fut: BoxFuture<'static, ()>| {
            invocations_for_spawner.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || futures::executor::block_on(fut));
        });

        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(spawner);
        let items = dummy_stream(vec![vec![0xcc]]);
        manager.register("p:1".to_string(), "demo_method", items, transport_dyn);

        // Wait for the worker future to drain to completion so we know
        // the spawner closure ran on this path.
        let _ = transport_typed.wait_for(2, std::time::Duration::from_secs(2));
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            1,
            "spawner must be invoked exactly once per register",
        );
    }

    /// A `_stop` arriving before `activate` (the stop-before-register race on
    /// non-serialized transports) must abort the subscription: no `_receive`
    /// frames are emitted even though the stream had items to yield.
    #[test]
    fn stop_before_activate_aborts_subscription() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());
        let token = manager.reserve("p:1".to_string());
        manager.handle_stop("p:1");
        let items = dummy_stream(vec![vec![0x01], vec![0x02]]);
        manager.activate(token, "demo_method", items, transport_dyn);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            transport_typed.sent().is_empty(),
            "a stop before activate must abort the subscription"
        );
    }

    /// Re-using a live request id (the duplicate-`_start` case) supersedes the
    /// previous subscription rather than leaking it: the first stream is
    /// stopped, only the second runs, and the superseded stream leaves no
    /// frames behind.
    #[test]
    fn duplicate_start_supersedes_previous_without_leak() {
        let transport_typed = Arc::new(RecordingTransport::new());
        let transport_dyn: Arc<dyn Transport> = transport_typed.clone();
        let manager = SubscriptionManager::new(thread_per_subscription_spawner());

        // First subscription never yields; the second reservation for the
        // same id must stop it.
        let pending: SubscriptionStream = Box::pin(stream::pending());
        manager.register(
            "p:1".to_string(),
            "demo_method",
            pending,
            transport_dyn.clone(),
        );

        // Second subscription yields one item then ends.
        let items = dummy_stream(vec![vec![0xaa]]);
        manager.register("p:1".to_string(), "demo_method", items, transport_dyn);

        // Exactly the second stream's frames appear: one receive + one
        // completion interrupt. The first (pending) stream contributes none.
        let observed = transport_typed.wait_for(2, std::time::Duration::from_secs(2));
        assert_eq!(
            observed, 2,
            "expected the second stream's receive + interrupt only"
        );
        let frames = transport_typed.sent();
        assert_eq!(frames[0].payload.tag, "demo_method_receive");
        assert_eq!(frames[0].payload.value, vec![0xaa]);
        assert_eq!(frames[1].payload.tag, "demo_method_interrupt");

        manager.handle_stop("p:1");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            transport_typed.sent().len(),
            2,
            "no leaked frames from the superseded stream"
        );
    }
}
