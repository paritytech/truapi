//! Live Paseo tests, excluded from CI. Run manually with:
//!
//! ```text
//! cargo test -p truapi-provider --features smoldot -- --ignored
//! ```
//!
//! The light-client test reads the chain-spec path from `PASEO_CHAIN_SPEC`.

#![cfg(all(feature = "ws", not(target_arch = "wasm32")))]

use std::time::Duration;

use futures::stream::StreamExt;
use serde_json::Value;
use truapi_platform::ChainProvider;
use truapi_provider::{ChainSource, NativeChainProvider};

const PASEO_GENESIS: [u8; 32] = [0; 32]; // Registry key only; not validated.
const PASEO_WS_URL: &str = "wss://paseo.rpc.amforc.com";

async fn follow_initializes(source: ChainSource) {
    let provider = NativeChainProvider::builder()
        .chain(PASEO_GENESIS, source)
        .build();
    let connection = provider
        .connect(PASEO_GENESIS)
        .await
        .expect("connecting to Paseo succeeds");
    let mut responses = connection.responses();
    connection.send(
        r#"{"jsonrpc":"2.0","id":1,"method":"chainHead_v1_follow","params":[false]}"#.to_owned(),
    );

    let initialized = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let frame = responses.next().await.expect("the connection stays alive");
            let frame: Value = serde_json::from_str(&frame).expect("frames are valid JSON");
            if frame["params"]["result"]["event"] == "initialized" {
                return frame;
            }
        }
    })
    .await
    .expect("the follow reaches initialized in time");

    assert!(
        initialized["params"]["result"]["finalizedBlockHashes"]
            .as_array()
            .is_some_and(|hashes| !hashes.is_empty()),
        "initialized must carry finalized hashes"
    );
    connection.close();
}

#[tokio::test]
#[ignore = "requires network access to Paseo"]
async fn ws_follow_initializes() {
    let url = url::Url::parse(PASEO_WS_URL).expect("static URL parses");
    follow_initializes(ChainSource::rpc_node(url)).await;
}

#[cfg(feature = "smoldot")]
#[tokio::test]
#[ignore = "requires network access to Paseo and PASEO_CHAIN_SPEC"]
async fn light_follow_initializes() {
    let path = std::env::var("PASEO_CHAIN_SPEC")
        .expect("set PASEO_CHAIN_SPEC to a Paseo relay chain-spec path");
    let spec = std::fs::read_to_string(path).expect("the chain spec is readable");
    follow_initializes(ChainSource::light_client(spec)).await;
}
