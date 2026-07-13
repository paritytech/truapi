//! Runs the same chainHead_v1 flow over both provider backends against the
//! Paseo relay chain: follow the chain, wait for `initialized`, read the
//! `Timestamp::Now` storage entry at the finalized block, and print it as a
//! wall-clock time.
//!
//! Usage:
//!
//! ```text
//! cargo run -p truapi-provider --features smoldot --example paseo_demo -- ws [WS_URL]
//! cargo run -p truapi-provider --features smoldot --example paseo_demo -- light CHAIN_SPEC_PATH
//! ```
//!
//! A Paseo relay chain spec is available at
//! <https://github.com/paritytech/smoldot/blob/main/demo-chain-specs/paseo.json>.
//! The light-client leg warp-syncs from scratch and can take tens of seconds.

// The example is native-only; the wasm build gets a stub main so `cargo test
// --target wasm32-unknown-unknown` can still build every target.
#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    imp::run().await;
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::time::{Duration, Instant, UNIX_EPOCH};

    use futures::stream::{BoxStream, StreamExt};
    use serde_json::{Value, json};
    use truapi_platform::ChainProvider;
    use truapi_provider::{ChainSource, NativeChainProvider};

    /// Paseo relay-chain genesis hash.
    const PASEO_GENESIS_HEX: &str =
        "77afd6190f1554ad45fd0d31aee62aacc33c6db0ea801129acb813f913e0764f";

    /// Public Paseo WebSocket endpoint used when none is given.
    const DEFAULT_WS_URL: &str = "wss://paseo.rpc.amforc.com";

    /// Storage key of `Timestamp::Now` (`twox128("Timestamp") ++ twox128("Now")`).
    const TIMESTAMP_NOW_KEY: &str =
        "0xf0c365c3cf59d671eb72da0e7a4113c49f1f0515f462cdcf84e0f1d6045dfcbb";

    const STEP_TIMEOUT: Duration = Duration::from_secs(300);

    pub(super) async fn run() {
        let mut args = std::env::args().skip(1);
        let mode = args.next().unwrap_or_else(|| usage());
        let source = match mode.as_str() {
            "ws" => {
                let url = args.next().unwrap_or_else(|| DEFAULT_WS_URL.to_owned());
                let url = url::Url::parse(&url).expect("the WS URL must parse");
                ChainSource::rpc_node(url)
            }
            "light" => {
                let path = args.next().unwrap_or_else(|| usage());
                let spec = std::fs::read_to_string(&path).expect("the chain spec must be readable");
                ChainSource::light_client(spec)
            }
            _ => usage(),
        };

        let genesis = parse_genesis(PASEO_GENESIS_HEX);
        let provider = NativeChainProvider::builder()
            .chain(genesis, source)
            .build();

        let started = Instant::now();
        let connection = provider
            .connect(genesis)
            .await
            .expect("connecting to Paseo must succeed");
        println!("[{mode}] connected in {:?}", started.elapsed());

        let mut responses = connection.responses();

        connection.send(
            json!({
                "jsonrpc": "2.0",
                "id": "genesis",
                "method": "chainSpec_v1_genesisHash",
                "params": [],
            })
            .to_string(),
        );
        let live_genesis = wait_for(&mut responses, |frame| {
            (frame["id"] == "genesis").then(|| frame["result"].as_str().map(str::to_owned))?
        })
        .await;
        assert_eq!(
            live_genesis.trim_start_matches("0x"),
            PASEO_GENESIS_HEX,
            "the endpoint serves a different chain than the Paseo genesis constant"
        );
        println!("[{mode}] genesis hash verified: {live_genesis}");

        connection.send(
            json!({
                "jsonrpc": "2.0",
                "id": "follow",
                "method": "chainHead_v1_follow",
                "params": [false],
            })
            .to_string(),
        );
        let follow_response = wait_for(&mut responses, |frame| {
            (frame["id"] == "follow").then(|| frame["result"].as_str().map(str::to_owned))?
        })
        .await;
        println!("[{mode}] follow subscription: {follow_response}");

        let initialized = wait_for(&mut responses, |frame| {
            let event = follow_event(frame, &follow_response)?;
            println!("[{mode}] follow event: {}", event["event"]);
            (event["event"] == "initialized").then(|| event.clone())
        })
        .await;
        let finalized_hash = initialized["finalizedBlockHashes"]
            .as_array()
            .and_then(|hashes| hashes.last())
            .and_then(Value::as_str)
            .expect("initialized carries at least one finalized hash")
            .to_owned();
        println!(
            "[{mode}] initialized at {finalized_hash} after {:?}",
            started.elapsed()
        );

        // Prefer a best block when one shows up quickly: on a chain with lagging
        // finality, full nodes may have pruned the finalized block's state.
        let best_hash = try_wait_for(&mut responses, Duration::from_secs(10), |frame| {
            let event = follow_event(frame, &follow_response)?;
            (event["event"] == "bestBlockChanged")
                .then(|| event["bestBlockHash"].as_str().map(str::to_owned))?
        })
        .await;
        let target_hash = best_hash.unwrap_or_else(|| finalized_hash.clone());
        println!("[{mode}] reading storage at {target_hash}");

        let read_started = Instant::now();
        connection.send(
            json!({
                "jsonrpc": "2.0",
                "id": "storage",
                "method": "chainHead_v1_storage",
                "params": [
                    follow_response,
                    target_hash,
                    [{ "key": TIMESTAMP_NOW_KEY, "type": "value" }],
                    null,
                ],
            })
            .to_string(),
        );
        let value = wait_for(&mut responses, |frame| {
            let event = follow_event(frame, &follow_response)?;
            match event["event"].as_str() {
                Some("operationInaccessible") | Some("operationError") => {
                    panic!("the storage read failed: {event}")
                }
                Some("operationStorageItems") => event["items"]
                    .as_array()
                    .and_then(|items| items.first())
                    .and_then(|item| item["value"].as_str())
                    .map(str::to_owned),
                _ => None,
            }
        })
        .await;
        println!(
            "[{mode}] Timestamp::Now = {value} (read took {:?})",
            read_started.elapsed()
        );

        let millis = u64::from_le_bytes(
            parse_hex(&value)
                .try_into()
                .expect("Timestamp::Now is a u64"),
        );
        let timestamp = UNIX_EPOCH + Duration::from_millis(millis);
        println!(
            "[{mode}] on-chain time: {timestamp:?} (total {:?})",
            started.elapsed()
        );

        connection.send(
            json!({
                "jsonrpc": "2.0",
                "id": "unfollow",
                "method": "chainHead_v1_unfollow",
                "params": [follow_response],
            })
            .to_string(),
        );
        connection.close();
    }

    fn usage() -> ! {
        eprintln!("usage: paseo_demo ws [WS_URL] | paseo_demo light CHAIN_SPEC_PATH");
        std::process::exit(2);
    }

    /// Extract the follow-event payload if `frame` is a `chainHead_v1_followEvent`
    /// notification for `subscription`.
    fn follow_event<'a>(frame: &'a Value, subscription: &str) -> Option<&'a Value> {
        (frame["method"] == "chainHead_v1_followEvent"
            && frame["params"]["subscription"] == subscription)
            .then(|| &frame["params"]["result"])
    }

    /// Read frames until `pick` extracts a value; panics on stream end or timeout.
    async fn wait_for<T>(
        responses: &mut BoxStream<'static, String>,
        pick: impl FnMut(&Value) -> Option<T>,
    ) -> T {
        try_wait_for(responses, STEP_TIMEOUT, pick)
            .await
            .expect("timed out waiting for a JSON-RPC frame")
    }

    /// Read frames until `pick` extracts a value or `limit` elapses.
    async fn try_wait_for<T>(
        responses: &mut BoxStream<'static, String>,
        limit: Duration,
        mut pick: impl FnMut(&Value) -> Option<T>,
    ) -> Option<T> {
        let deadline = Instant::now() + limit;
        loop {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let frame = tokio::time::timeout(remaining, responses.next())
                .await
                .ok()?
                .expect("the connection ended unexpectedly");
            let frame: Value = serde_json::from_str(&frame).expect("frames are valid JSON");
            if let Some(value) = pick(&frame) {
                return Some(value);
            }
        }
    }

    fn parse_genesis(hex_str: &str) -> [u8; 32] {
        hex::decode(hex_str)
            .expect("the genesis constant is valid hex")
            .try_into()
            .expect("the genesis constant is 32 bytes")
    }

    fn parse_hex(value: &str) -> Vec<u8> {
        hex::decode(value.trim_start_matches("0x")).expect("storage values are valid hex")
    }
}
