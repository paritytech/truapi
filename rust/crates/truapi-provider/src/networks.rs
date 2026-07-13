//! Bundled network catalog.
//!
//! A network is a relay chain plus its system parachains, with the relay
//! topology and statement-store placement fixed here so hosts register a whole
//! network by name instead of assembling specs, relay wiring, and genesis
//! hashes themselves. Only networks that ship a light-sync checkpoint are
//! bundled (a checkpointless spec carries full genesis storage — megabytes
//! unfit for a light-client binary); other networks are supplied per chain via
//! [`ChainSource::light_client`](crate::ChainSource::light_client).

use truapi::latest::GenericError;

use crate::config::ChainSource;
use crate::provider::NativeChainProviderBuilder;

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
    asset_hub: ChainDef,
    bulletin: ChainDef,
    people: ChainDef,
}

/// Genesis hashes of a registered network's chains, returned by
/// [`NativeChainProviderBuilder::add_network`] so the host knows what to
/// [`connect`](crate::NativeChainProvider) to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkChains {
    /// Relay-chain genesis hash.
    pub relay: [u8; 32],
    /// Asset Hub genesis hash.
    pub asset_hub: [u8; 32],
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
    asset_hub: ChainDef {
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

/// Register every chain of the bundled network `name`, wiring the parachains
/// to the relay and enabling the statement-store protocol where the catalog
/// specifies it. Returns the chains' genesis hashes.
pub(crate) fn add_network(
    builder: NativeChainProviderBuilder,
    name: &str,
) -> Result<(NativeChainProviderBuilder, NetworkChains), GenericError> {
    let network = CATALOG
        .iter()
        .find(|network| network.name == name)
        .ok_or_else(|| GenericError {
            reason: format!(
                "unknown network \"{name}\"; bundled: {}",
                known_networks().collect::<Vec<_>>().join(", ")
            ),
        })?;

    let chains = NetworkChains {
        relay: genesis(&network.relay)?,
        asset_hub: genesis(&network.asset_hub)?,
        bulletin: genesis(&network.bulletin)?,
        people: genesis(&network.people)?,
    };

    let relay_source = light_source(&network.relay, None);
    let builder = builder
        .chain(chains.relay, relay_source)
        .chain(
            chains.asset_hub,
            light_source(&network.asset_hub, Some(chains.relay)),
        )
        .chain(
            chains.bulletin,
            light_source(&network.bulletin, Some(chains.relay)),
        )
        .chain(
            chains.people,
            light_source(&network.people, Some(chains.relay)),
        );

    Ok((builder, chains))
}

fn light_source(chain: &ChainDef, relay: Option<[u8; 32]>) -> ChainSource {
    let mut source = ChainSource::light_client(chain.spec);
    if let Some(relay) = relay {
        source = source.with_relay(relay);
    }
    if !chain.statement_protocol {
        source = source.without_statement_protocol();
    }
    source
}

impl NativeChainProviderBuilder {
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
        let (_, chains) = NativeChainProviderBuilder::new()
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
        let error = NativeChainProviderBuilder::new()
            .add_network("mainnet")
            .expect_err("an unbundled network must fail");
        assert!(error.reason.contains("paseo-next-v2"));
    }
}
