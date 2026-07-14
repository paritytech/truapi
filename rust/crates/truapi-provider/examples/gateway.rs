//! Local WebSocket gateway exposing [`EmbeddedChainProvider`] chains to a
//! browser host: each configured chain is served at `ws://LISTEN/<name>`, and
//! every inbound WebSocket connection becomes one provider connection.
//!
//! dotli's `rpc-gateway` backend can point at this process via its
//! `dotli:gateway-rpc-base` setting (e.g. `ws://127.0.0.1:9944`), which routes
//! the host's relay/asset-hub/people traffic here — light-client-verified
//! where a chain runs on the embedded smoldot, proxied where it targets a
//! remote node.
//!
//! Usage:
//!
//! ```text
//! cargo run -p truapi-provider --features networks --example gateway -- CONFIG.json
//! ```
//!
//! Config shape: each chain is a light client resolved from the bundled network
//! catalog by its genesis hash (relay wiring included), or a proxy to a remote
//! node via `url`:
//!
//! ```json
//! {
//!   "listen": "127.0.0.1:9944",
//!   "chains": {
//!     "relay": { "genesis": "0x…" },
//!     "asset-hub": { "genesis": "0x…", "url": "wss://node.example" }
//!   }
//! }
//! ```

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
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use futures::{SinkExt, StreamExt};
    use serde_json::Value;
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use truapi_platform::ChainProvider;
    use truapi_provider::{ChainSource, EmbeddedChainProvider};

    pub(super) async fn run() {
        let config_path = std::env::args().nth(1).unwrap_or_else(|| usage());
        let config: Value = serde_json::from_str(
            &std::fs::read_to_string(&config_path).expect("the config file must be readable"),
        )
        .expect("the config file must be valid JSON");

        let listen = config["listen"]
            .as_str()
            .unwrap_or("127.0.0.1:9944")
            .to_owned();
        let chains = config["chains"]
            .as_object()
            .expect("config.chains must be an object");

        let mut builder = EmbeddedChainProvider::builder();
        let mut routes = HashMap::new();
        for (name, entry) in chains {
            let genesis = parse_genesis(
                entry["genesis"]
                    .as_str()
                    .expect("chain.genesis is required"),
            );
            // A `url` entry is proxied to a remote node; every other entry is a
            // light client the catalog resolves from its genesis hash — relay
            // wiring for parachains comes from the catalog, not this config.
            if let Some(url) = entry["url"].as_str() {
                println!("[gateway] /{name}: proxy to {url}");
                builder = builder.chain(
                    genesis,
                    ChainSource::rpc_node(url::Url::parse(url).expect("chain.url must parse")),
                );
            } else {
                println!("[gateway] /{name}: catalog light client");
            }
            routes.insert(format!("/{name}"), genesis);
        }

        let provider = Arc::new(builder.build());
        let routes = Arc::new(routes);
        let listener = TcpListener::bind(&listen)
            .await
            .expect("the gateway must bind its listen address");
        println!("[gateway] listening on ws://{listen}");

        loop {
            let (stream, peer) = listener.accept().await.expect("accept must succeed");
            tokio::spawn(serve(
                stream,
                peer,
                Arc::clone(&provider),
                Arc::clone(&routes),
            ));
        }
    }

    /// Bridge one WebSocket connection to one provider connection.
    async fn serve(
        stream: TcpStream,
        peer: SocketAddr,
        provider: Arc<EmbeddedChainProvider>,
        routes: Arc<HashMap<String, [u8; 32]>>,
    ) {
        let mut path = String::new();
        let websocket = match tokio_tungstenite::accept_hdr_async(
            stream,
            // The callback signature (and its large Err variant) is fixed by
            // tungstenite's accept_hdr_async.
            #[allow(clippy::result_large_err)]
            |request: &Request, response: Response| {
                path = request.uri().path().to_owned();
                Ok(response)
            },
        )
        .await
        {
            Ok(websocket) => websocket,
            Err(err) => {
                eprintln!("[gateway] {peer}: handshake failed: {err}");
                return;
            }
        };

        let Some(genesis) = routes.get(&path) else {
            eprintln!("[gateway] {peer}: unknown route {path}");
            return;
        };
        let connection = match provider.connect(*genesis).await {
            Ok(connection) => connection,
            Err(err) => {
                eprintln!(
                    "[gateway] {peer}: connect for {path} failed: {}",
                    err.reason
                );
                return;
            }
        };
        println!("[gateway] {peer}: connected to {path}");

        let (mut outbound, mut inbound) = websocket.split();
        let mut responses = connection.responses();
        loop {
            tokio::select! {
                frame = inbound.next() => match frame {
                    Some(Ok(Message::Text(text))) => connection.send(text),
                    Some(Ok(Message::Binary(bytes))) => match String::from_utf8(bytes) {
                        Ok(text) => connection.send(text),
                        Err(_) => eprintln!("[gateway] {peer}: dropping non-UTF-8 frame"),
                    },
                    Some(Ok(Message::Close(_))) | None => break,
                    // Ping/pong is answered by tungstenite itself.
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        eprintln!("[gateway] {peer}: receive failed: {err}");
                        break;
                    }
                },
                response = responses.next() => match response {
                    Some(text) => {
                        if outbound.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }

        connection.close();
        let _ = outbound.close().await;
        println!("[gateway] {peer}: disconnected from {path}");
    }

    fn usage() -> ! {
        eprintln!("usage: gateway CONFIG.json");
        std::process::exit(2);
    }

    fn parse_genesis(hex_str: &str) -> [u8; 32] {
        hex::decode(hex_str.trim_start_matches("0x"))
            .expect("genesis hashes are valid hex")
            .try_into()
            .expect("genesis hashes are 32 bytes")
    }
}
