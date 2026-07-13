//! WebSocket backend integration tests against an in-process jsonrpsee server.

#![cfg(all(feature = "ws", not(target_arch = "wasm32")))]

use std::net::SocketAddr;
use std::time::Duration;

use futures::stream::StreamExt;
use jsonrpsee::core::SubscriptionResult;
use jsonrpsee::server::{RpcModule, Server, ServerHandle, SubscriptionMessage};
use serde_json::Value;
use truapi_platform::ChainProvider;
use truapi_provider::{ChainSource, EmbeddedChainProvider};

const GENESIS: [u8; 32] = [7; 32];

async fn spawn_server() -> (SocketAddr, ServerHandle) {
    let server = Server::builder()
        .build("127.0.0.1:0")
        .await
        .expect("loopback server binds");
    let addr = server.local_addr().expect("server has a local address");

    let mut module = RpcModule::new(());
    module
        .register_method("echo", |params, (), _extensions| {
            params.one::<String>().unwrap_or_default()
        })
        .expect("echo registers");
    module
        .register_subscription(
            "sub_ticks",
            "tick",
            "unsub_ticks",
            |_params, pending, _context, _extensions| async move {
                let sink = pending.accept().await?;
                for tick in 0..3u32 {
                    sink.send(SubscriptionMessage::from_json(&tick)?).await?;
                }
                SubscriptionResult::Ok(())
            },
        )
        .expect("sub_ticks registers");

    (addr, server.start(module))
}

async fn connect(addr: SocketAddr) -> Box<dyn truapi_platform::JsonRpcConnection> {
    let url = url::Url::parse(&format!("ws://{addr}")).expect("loopback URL parses");
    EmbeddedChainProvider::builder()
        .chain(GENESIS, ChainSource::rpc_node(url))
        .build()
        .connect(GENESIS)
        .await
        .expect("loopback connect succeeds")
}

#[tokio::test]
async fn request_response_round_trip() {
    let (addr, _server) = spawn_server().await;
    let connection = connect(addr).await;
    let mut responses = connection.responses();

    connection.send(r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":["ping"]}"#.to_owned());
    let frame: Value =
        serde_json::from_str(&responses.next().await.expect("the echo response arrives"))
            .expect("frames are valid JSON");
    assert_eq!(frame["id"], 1);
    assert_eq!(frame["result"], "ping");
}

#[tokio::test]
async fn subscription_notifications_pass_through_raw() {
    let (addr, _server) = spawn_server().await;
    let connection = connect(addr).await;
    let mut responses = connection.responses();

    connection.send(r#"{"jsonrpc":"2.0","id":2,"method":"sub_ticks","params":[]}"#.to_owned());
    let ack: Value = serde_json::from_str(
        &responses
            .next()
            .await
            .expect("the subscription ack arrives"),
    )
    .expect("frames are valid JSON");
    assert_eq!(ack["id"], 2);
    let subscription = ack["result"].clone();

    for expected in 0..3u32 {
        let frame: Value =
            serde_json::from_str(&responses.next().await.expect("tick notifications arrive"))
                .expect("frames are valid JSON");
        assert_eq!(frame["method"], "tick");
        assert_eq!(frame["params"]["subscription"], subscription);
        assert_eq!(frame["params"]["result"], expected);
    }
}

#[tokio::test]
async fn server_shutdown_ends_the_stream() {
    let (addr, server) = spawn_server().await;
    let connection = connect(addr).await;
    let mut responses = connection.responses();

    server.stop().expect("server stops");
    server.stopped().await;

    let end = tokio::time::timeout(Duration::from_secs(10), responses.next()).await;
    assert_eq!(end.expect("the stream ends after server shutdown"), None);
}

#[tokio::test]
async fn close_ends_the_stream() {
    let (addr, _server) = spawn_server().await;
    let connection = connect(addr).await;
    let mut responses = connection.responses();

    connection.close();
    connection.close();
    assert_eq!(responses.next().await, None);
    // A late send must not panic on the closed connection.
    connection.send(r#"{"jsonrpc":"2.0","id":3,"method":"echo","params":["late"]}"#.to_owned());
}

#[tokio::test]
async fn connections_are_independent() {
    let (addr, _server) = spawn_server().await;
    let first = connect(addr).await;
    let second = connect(addr).await;
    let mut second_responses = second.responses();

    first.send(r#"{"jsonrpc":"2.0","id":4,"method":"echo","params":["mine"]}"#.to_owned());
    let cross_talk =
        tokio::time::timeout(Duration::from_millis(500), second_responses.next()).await;
    assert!(
        cross_talk.is_err(),
        "the second connection must not observe the first connection's response"
    );
}
