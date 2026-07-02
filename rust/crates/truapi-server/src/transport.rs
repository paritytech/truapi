//! Transport abstraction over platform-specific IPC mechanisms.

use crate::frame::ProtocolMessage;

/// A raw message pipe. Platform-specific implementations provide this.
pub trait Transport: Send + Sync {
    /// Send a protocol message to the other side.
    fn send(&self, message: ProtocolMessage);

    /// Register a handler for incoming messages. Returns an unsubscribe handle.
    fn on_message(&self, handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>) -> Box<dyn FnOnce()>;
}
