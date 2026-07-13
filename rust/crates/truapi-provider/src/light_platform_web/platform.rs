// Copyright 2019-2026 Parity Technologies (UK) Ltd.
// This file is dual-licensed as Apache-2.0 or GPL-3.0; see LICENSE-APACHE.
// Vendored from subxt-lightclient 0.50.1 (src/platform/wasm_platform.rs), used under Apache-2.0.

use super::socket::WasmSocket;

use core::{
    fmt::{self, Write as _},
    net::IpAddr,
    time::Duration,
};
use futures::prelude::*;
use smoldot::libp2p::with_buffers;
use smoldot_light::platform::{
    Address, ConnectionType, LogLevel, MultiStreamAddress, MultiStreamWebRtcConnection,
    PlatformRef, SubstreamDirection,
};

use std::{io, net::SocketAddr, pin::Pin};

const LOG_TARGET: &str = "subxt-platform-wasm";

/// Browser [`PlatformRef`] backing the embedded light client: tasks run on the
/// JS event loop and peers are dialed over the browser `WebSocket`.
#[derive(Clone)]
pub struct SubxtPlatform {}

impl SubxtPlatform {
    /// Create a browser platform. Stateless: all per-connection state lives in
    /// the sockets it opens.
    pub fn new() -> Self {
        SubxtPlatform {}
    }
}

impl PlatformRef for SubxtPlatform {
    type Delay = super::helpers::Delay;
    type Instant = super::helpers::Instant;
    type MultiStream = std::convert::Infallible;
    type Stream = super::helpers::Stream;
    type StreamConnectFuture = future::Ready<Self::Stream>;
    type MultiStreamConnectFuture = future::Pending<MultiStreamWebRtcConnection<Self::MultiStream>>;
    type ReadWriteAccess<'a> = with_buffers::ReadWriteAccess<'a, Self::Instant>;
    type StreamUpdateFuture<'a> = future::BoxFuture<'a, ()>;
    type StreamErrorRef<'a> = &'a std::io::Error;
    type NextSubstreamFuture<'a> = future::Pending<Option<(Self::Stream, SubstreamDirection)>>;

    fn now_from_unix_epoch(&self) -> Duration {
        super::helpers::now_from_unix_epoch()
    }

    fn now(&self) -> Self::Instant {
        super::helpers::now()
    }

    fn fill_random_bytes(&self, buffer: &mut [u8]) {
        // This could fail if the system does not have access to a good source of entropy.
        // Note: `rand::RngCore::fill_bytes` also panics on errors and `rand::OsCore` calls
        // identically into `getrandom::getrandom`.
        getrandom::getrandom(buffer).expect("Cannot fill random bytes");
    }

    fn sleep(&self, duration: Duration) -> Self::Delay {
        super::helpers::sleep(duration)
    }

    fn sleep_until(&self, when: Self::Instant) -> Self::Delay {
        self.sleep(when.saturating_duration_since(self.now()))
    }

    fn spawn_task(
        &self,
        _task_name: std::borrow::Cow<'_, str>,
        task: impl future::Future<Output = ()> + Send + 'static,
    ) {
        wasm_bindgen_futures::spawn_local(task);
    }

    fn client_name(&self) -> std::borrow::Cow<'_, str> {
        // Report this crate so peer/telemetry attribution names the client
        // that is actually connecting, consistent with the native
        // `DefaultPlatform` identity.
        env!("CARGO_PKG_NAME").into()
    }

    fn client_version(&self) -> std::borrow::Cow<'_, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn supports_connection_type(&self, connection_type: ConnectionType) -> bool {
        let result = matches!(
            connection_type,
            ConnectionType::WebSocketIpv4 { .. }
                | ConnectionType::WebSocketIpv6 { .. }
                | ConnectionType::WebSocketDns { .. }
        );

        tracing::trace!(
            target: LOG_TARGET,
            "Supports connection type={:?} result={}",
            connection_type, result
        );

        result
    }

    fn connect_stream(&self, multiaddr: Address) -> Self::StreamConnectFuture {
        tracing::trace!(target: LOG_TARGET, "Connect stream to multiaddr={:?}", multiaddr);

        // `PlatformRef` trait guarantees that `connect_stream` is only called with addresses
        // stated in `supports_connection_type`.
        let addr = match multiaddr {
            Address::WebSocketDns {
                hostname,
                port,
                secure: true,
            } => {
                format!("wss://{hostname}:{port}")
            }
            Address::WebSocketDns {
                hostname,
                port,
                secure: false,
            } => {
                format!("ws://{hostname}:{port}")
            }
            Address::WebSocketIp {
                ip: IpAddr::V4(ip),
                port,
            } => {
                let addr = SocketAddr::from((ip, port));
                format!("ws://{addr}")
            }
            Address::WebSocketIp {
                ip: IpAddr::V6(ip),
                port,
            } => {
                let addr = SocketAddr::from((ip, port));
                format!("ws://{addr}")
            }

            // The API user of the `PlatformRef` trait is never supposed to open connections of
            // a type that isn't supported.
            _ => {
                unreachable!(
                    "Connecting to an address not supported. This code path indicates a bug in smoldot. Please raise an issue at https://github.com/smol-dot/smoldot/issues"
                )
            }
        };

        let socket_future = async move {
            tracing::debug!(target: LOG_TARGET, "Connecting to addr={addr}");
            WasmSocket::new(addr.as_str()).map_err(|err| std::io::Error::other(err.to_string()))
        };

        future::ready(super::helpers::Stream(with_buffers::WithBuffers::new(
            Box::pin(socket_future),
        )))
    }

    fn connect_multistream(&self, _address: MultiStreamAddress) -> Self::MultiStreamConnectFuture {
        panic!(
            "Multistreams are not currently supported. This code path indicates a bug in smoldot. Please raise an issue at https://github.com/smol-dot/smoldot/issues"
        )
    }

    fn open_out_substream(&self, c: &mut Self::MultiStream) {
        // This function can only be called with so-called "multi-stream" connections. We never
        // open such connection.
        match *c {}
    }

    fn next_substream(&self, c: &'_ mut Self::MultiStream) -> Self::NextSubstreamFuture<'_> {
        // This function can only be called with so-called "multi-stream" connections. We never
        // open such connection.
        match *c {}
    }

    fn read_write_access<'a>(
        &self,
        stream: Pin<&'a mut Self::Stream>,
    ) -> Result<Self::ReadWriteAccess<'a>, &'a io::Error> {
        let stream = stream.project();
        stream.0.read_write_access(Self::Instant::now())
    }

    fn wait_read_write_again<'a>(
        &self,
        stream: Pin<&'a mut Self::Stream>,
    ) -> Self::StreamUpdateFuture<'a> {
        let stream = stream.project();
        Box::pin(stream.0.wait_read_write_again(|when| async move {
            let now = super::helpers::now();
            let duration = when.saturating_duration_since(now);
            super::helpers::sleep(duration).await;
        }))
    }

    fn log<'a>(
        &self,
        log_level: LogLevel,
        log_target: &'a str,
        message: &'a str,
        key_values: impl Iterator<Item = (&'a str, &'a dyn fmt::Display)>,
    ) {
        let mut message_build = String::with_capacity(128);
        message_build.push_str(message);
        let mut first = true;
        for (key, value) in key_values {
            if first {
                let _ = write!(message_build, "; ");
                first = false;
            } else {
                let _ = write!(message_build, ", ");
            }
            let _ = write!(message_build, "{key}={value}");
        }

        match log_level {
            LogLevel::Error => tracing::error!("target={log_target} {message_build}"),
            LogLevel::Warn => tracing::warn!("target={log_target} {message_build}"),
            LogLevel::Info => tracing::info!("target={log_target} {message_build}"),
            LogLevel::Debug => tracing::debug!("target={log_target} {message_build}"),
            LogLevel::Trace => tracing::trace!("target={log_target} {message_build}"),
        };
    }
}
