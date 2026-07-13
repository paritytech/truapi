//! Browser tests for the wasm32 WebSocket backend and the JS-facing API.
//!
//! Run headless (a local gateway provides the endpoint; the URL is baked in
//! at compile time via `TRUAPI_PROVIDER_TEST_WS`):
//!
//! ```text
//! cargo run -p truapi-provider --features smoldot --example gateway -- \
//!   rust/crates/truapi-provider/examples/gateway-dotli-paseo-next-v2.json &
//! TRUAPI_PROVIDER_TEST_WS=ws://127.0.0.1:9944/relay \
//!   wasm-pack test --headless --chrome rust/crates/truapi-provider --features js
//! ```
//!
//! Without the env var only the offline failure paths run.

#![cfg(all(target_arch = "wasm32", feature = "ws"))]

use futures::stream::StreamExt;
use truapi_platform::ChainProvider;
use truapi_provider::{ChainSource, EmbeddedChainProvider};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// Gateway route to test against, baked in at compile time.
const TEST_WS: Option<&str> = option_env!("TRUAPI_PROVIDER_TEST_WS");

const GENESIS: [u8; 32] = [7; 32];

fn provider(url: &str) -> EmbeddedChainProvider {
    let url = url::Url::parse(url).expect("test URL parses");
    EmbeddedChainProvider::builder()
        .chain(GENESIS, ChainSource::rpc_node(url))
        .build()
}

#[wasm_bindgen_test]
async fn unknown_genesis_is_an_error() {
    let error = provider("ws://127.0.0.1:1")
        .connect([9; 32])
        .await
        .err()
        .expect("connect must fail for an unregistered genesis");
    assert!(error.reason.contains(&"09".repeat(32)));
}

#[wasm_bindgen_test]
async fn handshake_failure_is_an_error() {
    let error = provider("ws://127.0.0.1:1")
        .connect(GENESIS)
        .await
        .err()
        .expect("connecting to a closed port must fail");
    assert!(
        error.reason.contains("WebSocket"),
        "unexpected: {}",
        error.reason
    );
}

#[wasm_bindgen_test]
async fn round_trip_and_close_against_gateway() {
    let Some(url) = TEST_WS else {
        return; // Offline run: covered by the failure-path tests above.
    };
    let connection = provider(url)
        .connect(GENESIS)
        .await
        .expect("connecting to the gateway succeeds");
    let mut responses = connection.responses();

    connection.send(
        r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}"#.to_owned(),
    );
    let response = responses.next().await.expect("the response arrives");
    assert!(response.contains("\"id\":1"), "unexpected: {response}");
    assert!(response.contains("0x"), "unexpected: {response}");

    connection.close();
    connection.close();
    assert_eq!(responses.next().await, None);
    // A late send must not panic on the closed connection.
    connection.send(r#"{"jsonrpc":"2.0","id":2,"method":"x","params":[]}"#.to_owned());
}

/// The embedded light client answers spec-local queries in the browser
/// without any network: proves the wasm smoldot platform end-to-end.
#[cfg(feature = "smoldot")]
#[wasm_bindgen_test]
async fn light_client_chain_name_round_trips_in_browser() {
    let provider = EmbeddedChainProvider::builder()
        .chain(
            GENESIS,
            ChainSource::light_client(include_str!("fixtures/paseo.json")),
        )
        .build();
    let connection = provider
        .connect(GENESIS)
        .await
        .expect("add_chain succeeds in the browser");
    let mut responses = connection.responses();
    connection.send(
        r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
    );
    let response = responses.next().await.expect("smoldot answers locally");
    assert!(
        response.contains("Paseo Testnet"),
        "unexpected response: {response}"
    );
    connection.close();
    assert_eq!(responses.next().await, None);
}

#[wasm_bindgen_test]
async fn js_api_round_trip_against_gateway() {
    use truapi_provider::js::{ChainProviderBuilder, Connection};

    let Some(url) = TEST_WS else {
        return;
    };
    let mut builder = ChainProviderBuilder::new();
    builder
        .add_rpc_chain(&format!("0x{}", "07".repeat(32)), url)
        .expect("chain registers");
    let handle = builder.build().expect("provider builds");

    let connection: Connection = handle
        .connect(&format!("0x{}", "07".repeat(32)))
        .await
        .expect("connect resolves");

    connection.send(
        r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}"#.to_owned(),
    );
    let response = connection
        .next_response()
        .await
        .expect("the response arrives");
    assert!(response.contains("\"id\":1"), "unexpected: {response}");

    connection.close();
    assert_eq!(
        connection.next_response().await,
        None,
        "the stream must end after close"
    );
}
