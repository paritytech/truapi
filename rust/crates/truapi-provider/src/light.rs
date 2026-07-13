//! Embedded smoldot light-client backend.
//!
//! One smoldot [`Client`] is shared per provider and created lazily on the
//! first light-client connect: the native platform spawns an OS thread pool,
//! while the wasm platform schedules on the JS event loop and dials peers
//! over the browser's `WebSocket`. Each connect adds the chain again: smoldot
//! deduplicates identical chains internally while giving every add its own
//! [`ChainId`], request queue, and response stream, which yields natural
//! per-connection isolation.
//!
//! Warm-start snapshots: send `chainHead_unstable_finalizedDatabase` over any
//! live connection, persist the returned string, and feed it back through
//! [`ChainSource::with_database`](crate::ChainSource::with_database).

use std::collections::HashMap;
use std::num::{NonZero, NonZeroUsize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures::stream::{self, BoxStream, StreamExt};
use smoldot_light::{
    AddChainConfig, AddChainConfigJsonRpc, ChainId, Client, JsonRpcResponses,
    network_service::StatementProtocolConfig,
};
use truapi::latest::GenericError;
use truapi_platform::JsonRpcConnection;

use crate::config::ChainSource;

/// The smoldot platform backing this target.
#[cfg(not(target_arch = "wasm32"))]
type Platform = Arc<smoldot_light::platform::DefaultPlatform>;
#[cfg(target_arch = "wasm32")]
type Platform = crate::light_platform_web::SubxtPlatform;

fn new_platform() -> Platform {
    #[cfg(not(target_arch = "wasm32"))]
    {
        smoldot_light::platform::DefaultPlatform::new(
            env!("CARGO_PKG_NAME").into(),
            env!("CARGO_PKG_VERSION").into(),
        )
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::light_platform_web::SubxtPlatform::new()
    }
}

/// Statement-store defaults mirroring smoldot's own example configuration.
const STATEMENT_MAX_SEEN: usize = 65_536;
const STATEMENT_FALSE_POSITIVE_RATE: f64 = 0.01;
const STATEMENT_AFFINITY_UPDATE_INTERVAL: Duration = Duration::from_secs(1);

/// JSON-RPC queue budgets for chains added by this backend.
const MAX_PENDING_REQUESTS: u32 = 128;
const MAX_SUBSCRIPTIONS: u32 = 1024;

struct LightInner {
    client: Client<Platform, ()>,
    /// Relay chains added implicitly for parachain entries, kept for the
    /// provider's lifetime so parachain connections can come and go freely.
    relays: HashMap<[u8; 32], ChainId>,
}

/// Lazily-started shared smoldot client owned by a provider.
pub(crate) struct LightState {
    inner: OnceLock<Arc<Mutex<LightInner>>>,
}

impl LightState {
    pub(crate) fn new() -> Self {
        LightState {
            inner: OnceLock::new(),
        }
    }

    fn inner(&self) -> &Arc<Mutex<LightInner>> {
        self.inner.get_or_init(|| {
            Arc::new(Mutex::new(LightInner {
                client: Client::new(new_platform()),
                relays: HashMap::new(),
            }))
        })
    }

    /// Add `source` (a [`ChainSource::LightClient`] entry) to the shared
    /// client and wrap it as a [`JsonRpcConnection`].
    pub(crate) async fn connect(
        &self,
        chains: &HashMap<[u8; 32], ChainSource>,
        source: &ChainSource,
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
        // `ChainSource` collapses to a single variant when only the smoldot
        // backend is enabled (e.g. the iOS build), making this match irrefutable.
        #[allow(irrefutable_let_patterns)]
        let ChainSource::LightClient {
            chain_spec,
            relay,
            database_content,
            statement_protocol,
        } = source
        else {
            return Err(GenericError {
                reason: "light backend invoked with a non-light chain source".to_owned(),
            });
        };

        let inner = Arc::clone(self.inner());
        let mut guard = inner.lock().expect("light-client mutex poisoned");

        let relay_id = match relay {
            None => None,
            Some(relay_genesis) => Some(add_relay(&mut guard, chains, *relay_genesis)?),
        };

        let success = guard
            .client
            .add_chain(AddChainConfig {
                user_data: (),
                specification: chain_spec,
                database_content: database_content.as_deref().unwrap_or(""),
                potential_relay_chains: relay_id.into_iter(),
                json_rpc: AddChainConfigJsonRpc::Enabled {
                    max_pending_requests: NonZero::new(MAX_PENDING_REQUESTS)
                        .expect("budget is non-zero"),
                    max_subscriptions: MAX_SUBSCRIPTIONS,
                },
                statement_protocol_config: statement_protocol.then(statement_protocol_config),
            })
            .map_err(|err| GenericError {
                reason: format!("failed to add chain to the light client: {err}"),
            })?;

        let responses = success
            .json_rpc_responses
            .expect("JSON-RPC was enabled for this chain");
        drop(guard);

        Ok(Box::new(LightConnection {
            inner,
            chain_id: success.chain_id,
            responses: Mutex::new(Some(responses)),
            closed: AtomicBool::new(false),
        }))
    }
}

/// Add the relay chain for a parachain entry, reusing an already-added one.
///
/// The relay is added with JSON-RPC disabled: it exists only so the parachain
/// can sync, and a direct connection to the relay genesis goes through its own
/// registry entry.
fn add_relay(
    guard: &mut LightInner,
    chains: &HashMap<[u8; 32], ChainSource>,
    relay_genesis: [u8; 32],
) -> Result<ChainId, GenericError> {
    if let Some(existing) = guard.relays.get(&relay_genesis) {
        return Ok(*existing);
    }

    let relay_hex = hex::encode(relay_genesis);
    let Some(ChainSource::LightClient {
        chain_spec,
        database_content,
        statement_protocol,
        ..
    }) = chains.get(&relay_genesis)
    else {
        return Err(GenericError {
            reason: format!("relay 0x{relay_hex} is not a registered light-client chain"),
        });
    };

    let success = guard
        .client
        .add_chain(AddChainConfig {
            user_data: (),
            specification: chain_spec,
            database_content: database_content.as_deref().unwrap_or(""),
            potential_relay_chains: std::iter::empty(),
            json_rpc: AddChainConfigJsonRpc::Disabled,
            statement_protocol_config: statement_protocol.then(statement_protocol_config),
        })
        .map_err(|err| GenericError {
            reason: format!("failed to add relay 0x{relay_hex} to the light client: {err}"),
        })?;

    guard.relays.insert(relay_genesis, success.chain_id);
    Ok(success.chain_id)
}

fn statement_protocol_config() -> StatementProtocolConfig {
    StatementProtocolConfig::new(
        NonZeroUsize::new(STATEMENT_MAX_SEEN).expect("budget is non-zero"),
        STATEMENT_FALSE_POSITIVE_RATE,
        statement_seed(),
        STATEMENT_AFFINITY_UPDATE_INTERVAL,
    )
}

/// Random bloom-filter seed from the target's entropy source.
fn statement_seed() -> u128 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rand::random()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).expect("the browser provides entropy");
        u128::from_le_bytes(bytes)
    }
}

/// One added smoldot chain exposed as a raw JSON-RPC pipe.
struct LightConnection {
    inner: Arc<Mutex<LightInner>>,
    chain_id: ChainId,
    responses: Mutex<Option<JsonRpcResponses<Platform>>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for LightConnection {
    fn send(&self, request: String) {
        // The chain-removal check and the request must happen under the same
        // lock: json_rpc_request panics on a removed ChainId.
        let mut guard = self.inner.lock().expect("light-client mutex poisoned");
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(err) = guard.client.json_rpc_request(request, self.chain_id) {
            tracing::warn!("light-client JSON-RPC request rejected: {err}");
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        match self
            .responses
            .lock()
            .expect("responses mutex poisoned")
            .take()
        {
            Some(responses) => stream::unfold(responses, |mut responses| async move {
                responses.next().await.map(|item| (item, responses))
            })
            .boxed(),
            None => stream::empty().boxed(),
        }
    }

    fn close(&self) {
        let mut guard = self.inner.lock().expect("light-client mutex poisoned");
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        // Removal also makes JsonRpcResponses::next() return None, ending an
        // already-taken responses stream.
        let _: () = guard.client.remove_chain(self.chain_id);
    }
}

impl Drop for LightConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use futures::stream::StreamExt;
    use truapi_platform::ChainProvider;

    use crate::{ChainSource, NativeChainProvider};

    /// Real relay-chain spec (checkpoint included) vendored from smoldot's
    /// demo specs: `add_chain` and spec-local JSON-RPC queries succeed without
    /// any network access.
    const RELAY_SPEC: &str = include_str!("../tests/fixtures/paseo.json");

    /// Parachain of [`RELAY_SPEC`], used to exercise the relay-add path.
    const PARACHAIN_SPEC: &str = include_str!("../tests/fixtures/paseo_people.json");

    const RELAY_GENESIS: [u8; 32] = [1; 32];
    const PARACHAIN_GENESIS: [u8; 32] = [2; 32];

    fn offline_provider() -> NativeChainProvider {
        NativeChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC))
            .build()
    }

    #[test]
    fn garbage_chain_spec_is_an_error() {
        let provider = NativeChainProvider::builder()
            .chain([1; 32], ChainSource::light_client("not a chain spec"))
            .build();
        let error = block_on(provider.connect([1; 32]))
            .err()
            .expect("a malformed chain spec must fail to connect");
        assert!(error.reason.contains("failed to add chain"));
    }

    #[test]
    fn unknown_relay_is_an_error() {
        let provider = NativeChainProvider::builder()
            .chain(
                RELAY_GENESIS,
                ChainSource::light_client(RELAY_SPEC).with_relay([9; 32]),
            )
            .build();
        let error = block_on(provider.connect(RELAY_GENESIS))
            .err()
            .expect("an unregistered relay must fail to connect");
        assert!(error.reason.contains("not a registered light-client chain"));
    }

    #[test]
    fn chain_name_round_trips_without_a_network() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();
        connection.send(
            r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
        let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
        assert!(
            response.contains("Paseo Testnet"),
            "unexpected response: {response}"
        );
    }

    #[test]
    fn parachain_reuses_its_registered_relay() {
        let provider = NativeChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC))
            .chain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC).with_relay(RELAY_GENESIS),
            )
            .build();
        // Two connects: the second must reuse the cached relay ChainId.
        for _ in 0..2 {
            let connection = block_on(provider.connect(PARACHAIN_GENESIS))
                .expect("parachain add_chain succeeds with its relay registered");
            let mut responses = connection.responses();
            connection.send(
                r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#
                    .to_owned(),
            );
            let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
            assert!(
                response.contains("Paseo People"),
                "unexpected response: {response}"
            );
        }
    }

    #[test]
    fn close_is_idempotent_and_ends_the_stream() {
        let provider = offline_provider();
        let connection =
            block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut responses = connection.responses();
        connection.close();
        connection.close();
        assert_eq!(block_on(responses.next()), None);
        // A late send must not panic on the removed chain.
        connection.send(
            r#"{"jsonrpc":"2.0","id":2,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
    }

    #[test]
    fn connections_to_the_same_chain_are_isolated() {
        let provider = offline_provider();
        let first = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let second = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
        let mut second_responses = second.responses();
        first.close();
        second.send(
            r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
        let response =
            block_on(second_responses.next()).expect("the second connection stays alive");
        assert!(
            response.contains("Paseo Testnet"),
            "unexpected response: {response}"
        );
    }
}
