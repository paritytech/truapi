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
//! [`LightClientBuilder::database`](crate::LightClientBuilder::database).
//!
//! Observability: on native targets smoldot logs through the `log` crate. A
//! host that wants those lines in its `tracing` output should install a
//! `log`->`tracing` bridge (e.g. `tracing_log::LogTracer`); the provider does
//! not install a global logger of its own.

use core::num::{NonZero, NonZeroUsize};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use smoldot_light::{
    AddChainConfig, AddChainConfigJsonRpc, ChainId, Client, HandleRpcError, JsonRpcResponses,
    network_service::StatementProtocolConfig,
};
use truapi_platform::JsonRpcConnection;

use crate::config::ChainSource;
use crate::error::{ProviderError, synthetic_error_frame};

/// Lock a mutex, recovering the guard if a previous holder panicked.
///
/// The embedded light client is a single process-wide instance shared by every
/// connection, so one poisoning event must not brick all of them. smoldot's own
/// calls under this lock do not panic; this is defense-in-depth for the shared
/// singleton's blast radius.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

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

/// JSON-RPC queue budgets for chains added by this backend. The provider is a
/// trusted in-process client, so the pending cap is generous (smoldot's docs
/// sanction up to `u32::MAX` for trusted callers); an overflow is still handled
/// gracefully by synthesizing an error response rather than hanging the caller.
const MAX_PENDING_REQUESTS: u32 = 1024;
const MAX_SUBSCRIPTIONS: u32 = 1024;

struct LightInner {
    client: Client<Platform, ()>,
    /// Relay chains added implicitly to sync parachain connections, refcounted
    /// by the live parachain connections that named them. A relay is removed
    /// from the client when its last such connection closes.
    relays: HashMap<[u8; 32], RelayEntry>,
}

/// A shared implicit relay chain and how many live parachain connections use it.
struct RelayEntry {
    chain_id: ChainId,
    refcount: usize,
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

    /// Number of implicit relay chains currently held.
    #[cfg(test)]
    pub(crate) fn relay_count(&self) -> usize {
        lock(self.inner()).relays.len()
    }

    /// Add `source` (a [`ChainSource::LightClient`] entry) to the shared
    /// client and wrap it as a [`JsonRpcConnection`].
    pub(crate) async fn connect(
        &self,
        chains: &HashMap<[u8; 32], ChainSource>,
        source: &ChainSource,
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
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
            return Err(ProviderError::Transport {
                reason: "light backend invoked with a non-light chain source".to_owned(),
            });
        };

        let inner = Arc::clone(self.inner());
        let mut guard = lock(&inner);

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
            .map_err(|err| ProviderError::AddChain {
                reason: err.to_string(),
            })?;

        let responses = success
            .json_rpc_responses
            .expect("JSON-RPC was enabled for this chain");
        drop(guard);

        // `send` synthesizes an error onto this channel when smoldot rejects a
        // request, so a full queue fails the caller fast instead of hanging.
        let (errors_tx, errors_rx) = mpsc::unbounded();

        Ok(Box::new(LightConnection {
            inner,
            chain_id: success.chain_id,
            relay: *relay,
            errors_tx,
            responses: Mutex::new(Some((responses, errors_rx))),
            closed: AtomicBool::new(false),
        }))
    }
}

/// Add the relay chain for a parachain entry, reusing an already-added one and
/// taking a reference on it for the calling connection.
///
/// The relay is added with JSON-RPC disabled: it exists only so the parachain
/// can sync, and a direct connection to the relay genesis goes through its own
/// registry entry.
fn add_relay(
    guard: &mut LightInner,
    chains: &HashMap<[u8; 32], ChainSource>,
    relay_genesis: [u8; 32],
) -> Result<ChainId, ProviderError> {
    if let Some(existing) = guard.relays.get_mut(&relay_genesis) {
        existing.refcount += 1;
        return Ok(existing.chain_id);
    }

    let Some(ChainSource::LightClient {
        chain_spec,
        database_content,
        statement_protocol,
        ..
    }) = chains.get(&relay_genesis)
    else {
        return Err(ProviderError::UnknownRelay {
            relay: relay_genesis,
        });
    };

    let success = guard
        .client
        .add_chain(AddChainConfig {
            user_data: (),
            specification: chain_spec,
            database_content: database_content.as_deref().unwrap_or(""),
            potential_relay_chains: core::iter::empty(),
            json_rpc: AddChainConfigJsonRpc::Disabled,
            statement_protocol_config: statement_protocol.then(statement_protocol_config),
        })
        .map_err(|err| ProviderError::AddChain {
            reason: err.to_string(),
        })?;

    guard.relays.insert(
        relay_genesis,
        RelayEntry {
            chain_id: success.chain_id,
            refcount: 1,
        },
    );
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
    /// Genesis of the implicit relay this connection holds a reference on, if
    /// it is a parachain; released on close.
    relay: Option<[u8; 32]>,
    /// Synthetic JSON-RPC error frames for requests smoldot rejected, merged
    /// into [`responses`](Self::responses) so the caller fails fast.
    errors_tx: mpsc::UnboundedSender<String>,
    /// Taken once by `responses()`: smoldot's response stream paired with the
    /// receiver for `errors_tx`.
    responses: Mutex<Option<(JsonRpcResponses<Platform>, mpsc::UnboundedReceiver<String>)>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for LightConnection {
    fn send(&self, request: String) {
        // The chain-removal check and the request must happen under the same
        // lock: json_rpc_request panics on a removed ChainId.
        let mut guard = lock(&self.inner);
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(HandleRpcError::TooManyPendingRequests { json_rpc_request }) =
            guard.client.json_rpc_request(request, self.chain_id)
        {
            // The connection stays alive (only this request is refused), so
            // synthesize an error for its id instead of dropping it silently.
            drop(guard);
            tracing::warn!("light-client request queue full; failing the request");
            if let Some(frame) =
                synthetic_error_frame(&json_rpc_request, "light client request queue full")
            {
                let _ = self.errors_tx.unbounded_send(frame);
            }
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        match lock(&self.responses).take() {
            Some((responses, errors)) => {
                let responses = stream::unfold(responses, |mut responses| async move {
                    responses.next().await.map(|item| (item, responses))
                });
                stream::select(responses, errors).boxed()
            }
            None => stream::empty().boxed(),
        }
    }

    fn close(&self) {
        let mut guard = lock(&self.inner);
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        // Removal makes JsonRpcResponses::next() return None; closing the error
        // channel ends its half of the merged stream, so `responses()`
        // terminates cleanly.
        let _: () = guard.client.remove_chain(self.chain_id);

        // Release the hold on the implicit relay and remove it once the last
        // parachain connection using it is gone. This connection's own chain is
        // already removed above, so no live chain still depends on the relay.
        if let Some(relay_genesis) = self.relay {
            let orphaned = match guard.relays.get_mut(&relay_genesis) {
                Some(entry) => {
                    entry.refcount -= 1;
                    (entry.refcount == 0).then_some(entry.chain_id)
                }
                None => None,
            };
            if let Some(relay_id) = orphaned {
                guard.relays.remove(&relay_genesis);
                let _: () = guard.client.remove_chain(relay_id);
            }
        }

        self.errors_tx.close_channel();
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

    use crate::{ChainSource, EmbeddedChainProvider};

    /// Real relay-chain spec (checkpoint included) vendored from smoldot's
    /// demo specs: `add_chain` and spec-local JSON-RPC queries succeed without
    /// any network access.
    const RELAY_SPEC: &str = include_str!("../tests/fixtures/paseo.json");

    /// Parachain of [`RELAY_SPEC`], used to exercise the relay-add path.
    const PARACHAIN_SPEC: &str = include_str!("../tests/fixtures/paseo_people.json");

    const RELAY_GENESIS: [u8; 32] = [1; 32];
    const PARACHAIN_GENESIS: [u8; 32] = [2; 32];

    fn offline_provider() -> EmbeddedChainProvider {
        EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .build()
    }

    #[test]
    fn garbage_chain_spec_is_an_error() {
        let provider = EmbeddedChainProvider::builder()
            .chain(
                [1; 32],
                ChainSource::light_client("not a chain spec").build(),
            )
            .build();
        let error = block_on(provider.connect([1; 32]))
            .err()
            .expect("a malformed chain spec must fail to connect");
        assert!(error.reason.contains("failed to add a chain"));
    }

    #[test]
    fn unknown_relay_is_an_error() {
        let provider = EmbeddedChainProvider::builder()
            .chain(
                RELAY_GENESIS,
                ChainSource::light_client(RELAY_SPEC).relay([9; 32]).build(),
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
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .chain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC)
                    .relay(RELAY_GENESIS)
                    .build(),
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
    fn relay_is_reclaimed_when_the_last_parachain_closes() {
        let provider = EmbeddedChainProvider::builder()
            .chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
            .chain(
                PARACHAIN_GENESIS,
                ChainSource::light_client(PARACHAIN_SPEC)
                    .relay(RELAY_GENESIS)
                    .build(),
            )
            .build();
        let first = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
        let second = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
        assert_eq!(
            provider.relay_count(),
            1,
            "both connections share one relay"
        );
        first.close();
        assert_eq!(
            provider.relay_count(),
            1,
            "the relay stays while a parachain connection is live"
        );
        second.close();
        assert_eq!(
            provider.relay_count(),
            0,
            "the relay is reclaimed after the last parachain connection closes"
        );
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
