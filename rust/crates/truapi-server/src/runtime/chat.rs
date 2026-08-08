//! Connection-scoped Chat streams shared by product and native entrypoints.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use truapi::Subscription;
use truapi::versioned::chat::HostChatActionSubscribeItem;

use crate::host_core::ProductRuntimeError;
#[cfg(any(test, not(target_arch = "wasm32")))]
const ACTION_BUFFER_CAPACITY: usize = 64;

/// Chat access policy shared by the wire runtime and the native entrypoints:
/// only a Chat-kind execution with an active session may use Chat, and the
/// host must have installed a native adapter.
pub(crate) fn chat_platform_for(
    execution_kind: truapi_platform::ProductExecutionKind,
    has_session: bool,
    chat: Option<&Arc<dyn truapi_platform::ChatPlatform>>,
) -> Result<Arc<dyn truapi_platform::ChatPlatform>, ProductRuntimeError> {
    if execution_kind != truapi_platform::ProductExecutionKind::Chat || !has_session {
        return Err(ProductRuntimeError::Denied);
    }
    chat.cloned().ok_or(ProductRuntimeError::Unsupported)
}

#[derive(Default)]
struct State {
    actions: Option<mpsc::UnboundedSender<HostChatActionSubscribeItem>>,
    action_buffer: VecDeque<HostChatActionSubscribeItem>,
    closed: bool,
}

/// Mutable Chat protocol state owned by one product connection.
pub(crate) struct ChatConnection {
    state: Arc<Mutex<State>>,
}

impl ChatConnection {
    /// Create empty Chat state for one product connection.
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    /// Open the product's action subscription and drain buffered actions first.
    pub(crate) fn subscribe_actions(&self) -> Subscription<HostChatActionSubscribeItem> {
        let (sender, receiver) = mpsc::unbounded();
        let mut state = self.state.lock().expect("chat state mutex poisoned");
        if state.closed {
            return Subscription::empty();
        }
        for item in state.action_buffer.drain(..) {
            let _ = sender.unbounded_send(item);
        }
        state.actions = Some(sender);
        Subscription::new(Box::pin(receiver))
    }

    /// Publish one native action, buffering it until the product subscribes.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(crate) fn publish_action(
        &self,
        mut action: HostChatActionSubscribeItem,
    ) -> Result<(), ProductRuntimeError> {
        let mut state = self.state.lock().expect("chat state mutex poisoned");
        if state.closed {
            return Err(ProductRuntimeError::Closed);
        }
        if let Some(sender) = state.actions.as_ref() {
            match sender.unbounded_send(action) {
                Ok(()) => return Ok(()),
                Err(error) => action = error.into_inner(),
            }
            state.actions = None;
        }
        if state.action_buffer.len() == ACTION_BUFFER_CAPACITY {
            return Err(ProductRuntimeError::BufferFull);
        }
        state.action_buffer.push_back(action);
        Ok(())
    }

    /// End the current subscriber's stream while keeping buffered actions for
    /// the next product connection that subscribes.
    pub(crate) fn detach(&self) {
        let mut state = self.state.lock().expect("chat state mutex poisoned");
        state.actions = None;
    }

    /// Close all connection-scoped Chat streams and discard buffered work.
    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(crate) fn close(&self) {
        let mut state = self.state.lock().expect("chat state mutex poisoned");
        state.closed = true;
        state.actions = None;
        state.action_buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::executor::block_on;
    use truapi::v01;
    use truapi::v01::{ChatActionPayload, ChatMessageContent};

    fn action(text: &str) -> HostChatActionSubscribeItem {
        HostChatActionSubscribeItem::V1(v01::HostChatActionSubscribeItem {
            room_id: "room".to_string(),
            peer: "alice".to_string(),
            payload: ChatActionPayload::MessagePosted(ChatMessageContent::Text {
                text: text.to_string(),
            }),
        })
    }

    fn connection() -> ChatConnection {
        ChatConnection::new()
    }

    #[test]
    fn buffered_actions_are_drained_in_fifo_order() {
        let connection = connection();
        connection.publish_action(action("first")).unwrap();
        connection.publish_action(action("second")).unwrap();

        let mut actions = connection.subscribe_actions();
        assert_eq!(block_on(actions.next()), Some(action("first")));
        assert_eq!(block_on(actions.next()), Some(action("second")));
    }

    #[test]
    fn full_startup_action_buffer_is_reported() {
        let connection = connection();
        for index in 0..ACTION_BUFFER_CAPACITY {
            connection
                .publish_action(action(&index.to_string()))
                .unwrap();
        }

        assert!(matches!(
            connection.publish_action(action("overflow")),
            Err(ProductRuntimeError::BufferFull)
        ));
    }

    #[test]
    fn detach_keeps_buffered_actions_for_the_next_subscriber() {
        let connection = connection();
        let mut first = connection.subscribe_actions();
        connection.publish_action(action("live")).unwrap();
        assert_eq!(block_on(first.next()), Some(action("live")));

        connection.detach();
        assert_eq!(block_on(first.next()), None);
        connection.publish_action(action("buffered")).unwrap();

        let mut second = connection.subscribe_actions();
        assert_eq!(block_on(second.next()), Some(action("buffered")));
    }

    #[test]
    fn closing_discards_buffered_actions() {
        let connection = connection();
        connection.publish_action(action("discard me")).unwrap();
        connection.close();

        let mut actions = connection.subscribe_actions();
        assert_eq!(block_on(actions.next()), None);
        assert!(matches!(
            connection.publish_action(action("too late")),
            Err(ProductRuntimeError::Closed)
        ));
    }

    #[test]
    fn separate_connections_cannot_observe_each_others_actions() {
        let first = connection();
        let second = connection();
        let mut first_actions = first.subscribe_actions();
        let mut second_actions = second.subscribe_actions();

        first.publish_action(action("first only")).unwrap();
        second.publish_action(action("second only")).unwrap();
        assert_eq!(block_on(first_actions.next()), Some(action("first only")));
        assert_eq!(block_on(second_actions.next()), Some(action("second only")));

        second.close();
        assert!(matches!(
            second.publish_action(action("closed")),
            Err(ProductRuntimeError::Closed)
        ));
    }
}
