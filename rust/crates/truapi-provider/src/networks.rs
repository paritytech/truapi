//! Bundled network catalog.
//!
//! A network is a relay chain plus its system parachains, with the relay
//! topology and statement-store placement fixed here so hosts register a whole
//! network by name instead of assembling specs, relay wiring, and genesis
//! hashes themselves. Only networks that ship a light-sync checkpoint are
//! bundled (a checkpointless spec carries full genesis storage — megabytes
//! unfit for a light-client binary); other networks are supplied per chain via
//! [`ChainSource::light_client`](crate::ChainSource::light_client).

use std::collections::HashMap;

use truapi::latest::GenericError;

use crate::config::ChainSource;
use crate::provider::EmbeddedChainProviderBuilder;

/// One chain within a [`NetworkDef`].
struct ChainDef {
    /// `0x`-prefixed genesis hash, the chain's stable identity.
    genesis_hex: &'static str,
    /// Chain-spec JSON.
    spec: &'static str,
    /// Whether the statement-store networking protocol runs on this chain.
    statement_protocol: bool,
}

/// A relay chain and its system parachains.
struct NetworkDef {
    name: &'static str,
    relay: ChainDef,
    assethub: ChainDef,
    bulletin: ChainDef,
    people: ChainDef,
}

/// Genesis hashes of a registered network's chains, returned by
/// [`EmbeddedChainProviderBuilder::add_network`] so the host knows what to
/// [`connect`](crate::EmbeddedChainProvider) to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkChains {
    /// Relay-chain genesis hash.
    pub relay: [u8; 32],
    /// Asset Hub genesis hash.
    pub assethub: [u8; 32],
    /// Bulletin-chain genesis hash.
    pub bulletin: [u8; 32],
    /// People-chain genesis hash.
    pub people: [u8; 32],
}

const CATALOG: &[NetworkDef] = &[NetworkDef {
    name: "paseo-next-v2",
    relay: ChainDef {
        genesis_hex: "0x77afd6190f1554ad45fd0d31aee62aacc33c6db0ea801129acb813f913e0764f",
        spec: include_str!("../networks/paseo.json"),
        statement_protocol: false,
    },
    assethub: ChainDef {
        genesis_hex: "0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f",
        spec: include_str!("../networks/paseo-next-v2-asset-hub.json"),
        statement_protocol: false,
    },
    bulletin: ChainDef {
        genesis_hex: "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
        spec: include_str!("../networks/paseo-next-v2-bulletin.json"),
        statement_protocol: false,
    },
    people: ChainDef {
        genesis_hex: "0xc5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5",
        spec: include_str!("../networks/paseo-next-v2-people.json"),
        statement_protocol: true,
    },
}];

/// Names of the bundled networks.
pub fn known_networks() -> impl Iterator<Item = &'static str> {
    CATALOG.iter().map(|network| network.name)
}

fn genesis(chain: &ChainDef) -> Result<[u8; 32], GenericError> {
    hex::decode(chain.genesis_hex.trim_start_matches("0x"))
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| GenericError {
            reason: format!("bundled genesis hash {} is malformed", chain.genesis_hex),
        })
}

/// A network's genesis hashes paired with each chain's genesis hash and
/// [`ChainSource`].
type NetworkSources = (NetworkChains, Vec<([u8; 32], ChainSource)>);

/// A resolved catalog network: chains keyed by genesis hash, plus the relay the
/// queried chain syncs through (`None` when it is itself the relay).
type ResolvedNetwork = (HashMap<[u8; 32], ChainSource>, Option<[u8; 32]>);

/// Genesis hashes and standalone per-chain [`ChainSource`]s of one network; the
/// caller pairs each parachain with the network's relay genesis.
fn network_sources(network: &NetworkDef) -> Result<NetworkSources, GenericError> {
    let chains = NetworkChains {
        relay: genesis(&network.relay)?,
        assethub: genesis(&network.assethub)?,
        bulletin: genesis(&network.bulletin)?,
        people: genesis(&network.people)?,
    };
    let sources = vec![
        (chains.relay, light_source(&network.relay)),
        (chains.assethub, light_source(&network.assethub)),
        (chains.bulletin, light_source(&network.bulletin)),
        (chains.people, light_source(&network.people)),
    ];
    Ok((chains, sources))
}

/// Register every chain of the bundled network `name`, wiring the parachains
/// to the relay and enabling the statement-store protocol where the catalog
/// specifies it. Returns the chains' genesis hashes.
pub(crate) fn add_network(
    builder: EmbeddedChainProviderBuilder,
    name: &str,
) -> Result<(EmbeddedChainProviderBuilder, NetworkChains), GenericError> {
    let network = CATALOG
        .iter()
        .find(|network| network.name == name)
        .ok_or_else(|| GenericError {
            reason: format!(
                "unknown network \"{name}\"; bundled: {}",
                known_networks().collect::<Vec<_>>().join(", ")
            ),
        })?;

    let (chains, sources) = network_sources(network)?;
    let relay_genesis = chains.relay;
    let builder = sources
        .into_iter()
        .fold(builder, |builder, (genesis_hash, source)| {
            if genesis_hash == relay_genesis {
                builder.chain(genesis_hash, source)
            } else {
                builder.parachain(genesis_hash, source, relay_genesis)
            }
        });
    Ok((builder, chains))
}

/// Resolve the bundled network containing `genesis_hash` from that hash alone:
/// its chains and the relay `genesis_hash` syncs through. `None` if no bundled
/// network defines it.
pub(crate) fn catalog_network_chains(genesis_hash: [u8; 32]) -> Option<ResolvedNetwork> {
    for network in CATALOG {
        let Ok((chains, sources)) = network_sources(network) else {
            continue;
        };
        if sources.iter().any(|(hash, _)| *hash == genesis_hash) {
            // The relay syncs on its own; a parachain syncs through the relay.
            let relay = (genesis_hash != chains.relay).then_some(chains.relay);
            return Some((sources.into_iter().collect(), relay));
        }
    }
    None
}

fn light_source(chain: &ChainDef) -> ChainSource {
    ChainSource::LightClient {
        specification: chain.spec.into(),
        database_content: None,
        statement_protocol: chain.statement_protocol,
    }
}

impl EmbeddedChainProviderBuilder {
    /// Register every chain of the bundled network `name` (see
    /// [`known_networks`]). Returns the builder and the network's genesis
    /// hashes. Errors when `name` is not bundled.
    pub fn add_network(self, name: &str) -> Result<(Self, NetworkChains), GenericError> {
        add_network(self, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paseo_next_v2_registers_four_chains_with_expected_hashes() {
        let (_, chains) = EmbeddedChainProviderBuilder::new()
            .add_network("paseo-next-v2")
            .expect("bundled network registers");
        assert_eq!(
            hex::encode(chains.relay),
            &CATALOG[0].relay.genesis_hex[2..]
        );
        assert_eq!(
            hex::encode(chains.people),
            &CATALOG[0].people.genesis_hex[2..]
        );
    }

    #[test]
    fn unknown_network_lists_the_catalog() {
        let error = EmbeddedChainProviderBuilder::new()
            .add_network("mainnet")
            .expect_err("an unbundled network must fail");
        assert!(error.reason.contains("paseo-next-v2"));
    }

    #[test]
    fn connect_resolves_network_from_genesis_alone() {
        use futures::executor::block_on;
        use futures::stream::StreamExt;
        use truapi_platform::ChainProvider;

        // An empty provider — no explicit registration — still connects to a
        // catalog chain from its genesis hash alone.
        let relay = genesis(&CATALOG[0].relay).expect("catalog genesis parses");
        let provider = crate::EmbeddedChainProvider::builder().build();
        let connection = block_on(provider.connect(relay))
            .expect("catalog resolves the relay genesis without registration");
        let mut responses = connection.responses();
        connection.send(
            r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
        );
        let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
        assert!(
            response.contains("\"Paseo\""),
            "unexpected response: {response}"
        );
    }
}
