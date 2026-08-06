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

const CATALOG: &[NetworkDef] = &[
    NetworkDef {
        name: "paseo-next-v2",
        relay: ChainDef {
            genesis_hex: "0x374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd",
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
    },
    NetworkDef {
        name: "previewnet",
        relay: ChainDef {
            genesis_hex: "0x8c27ddf678c2ae9bef0efebfc485a9309f3d735c6d3fbb8d947afc3ace0e80f4",
            spec: include_str!("../networks/previewnet.json"),
            statement_protocol: false,
        },
        assethub: ChainDef {
            genesis_hex: "0x4d11c803cc6921429e3876638977ad006ea1bba8cd3976a0bca2f164e7026210",
            spec: include_str!("../networks/previewnet-asset-hub.json"),
            statement_protocol: false,
        },
        bulletin: ChainDef {
            genesis_hex: "0x2778b1c94c4362e49a54be57d3056bc714f3712e4486625312704ffb74eb973d",
            spec: include_str!("../networks/previewnet-bulletin.json"),
            statement_protocol: false,
        },
        people: ChainDef {
            genesis_hex: "0x3138c6d4ce58c760047a413c2a930e919b4673a841ab4890de59aac3bd037f3d",
            spec: include_str!("../networks/previewnet-people.json"),
            statement_protocol: true,
        },
    },
];

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

/// The network name and service (`relay`/`asset-hub`/`bulletin`/`people`) that
/// `genesis_hash` maps to in the catalog, for log attribution. `None` if no
/// bundled network defines it.
pub(crate) fn catalog_service(genesis_hash: [u8; 32]) -> Option<(&'static str, &'static str)> {
    for network in CATALOG {
        for (service, chain) in [
            ("relay", &network.relay),
            ("asset-hub", &network.assethub),
            ("bulletin", &network.bulletin),
            ("people", &network.people),
        ] {
            if genesis(chain).ok() == Some(genesis_hash) {
                return Some((network.name, service));
            }
        }
    }
    None
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

    /// Every catalog entry's genesis hash must be the hash of the genesis block
    /// its own bundled spec describes.
    ///
    /// A spec refreshed against a chain that has since been re-genesised keeps
    /// working for the light client (smoldot derives the real hash from the
    /// spec), so the stale entry is invisible until a caller connects by the
    /// chain's true genesis hash and is told it is unknown. This derives the
    /// hash offline and catches that drift at the refresh.
    #[test]
    fn every_catalog_genesis_hash_matches_its_bundled_spec() {
        for network in CATALOG {
            for (service, chain) in [
                ("relay", &network.relay),
                ("asset-hub", &network.assethub),
                ("bulletin", &network.bulletin),
                ("people", &network.people),
            ] {
                let derived = genesis_hash_of_spec(chain.spec);
                assert_eq!(
                    format!("0x{}", hex::encode(derived)),
                    chain.genesis_hex,
                    "the {} {} spec describes a different chain than its catalog entry claims; \
                     set its genesis_hex to the left-hand hash",
                    network.name,
                    service,
                );
            }
        }
    }

    /// Hash of the genesis block a light chain spec describes.
    ///
    /// A light spec carries only the genesis state root, which is enough: the
    /// genesis block has no parent, number zero, no extrinsics and no digest,
    /// so its header is fully determined by that root.
    fn genesis_hash_of_spec(spec: &str) -> [u8; 32] {
        /// Merkle value of an empty trie, the genesis block's extrinsics root.
        const EMPTY_TRIE_ROOT: &str =
            "03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314";

        let spec: serde_json::Value = serde_json::from_str(spec).expect("bundled spec is JSON");
        let state_root = spec["genesis"]["stateRootHash"]
            .as_str()
            .expect("bundled spec carries a genesis state root");
        let state_root =
            hex::decode(state_root.trim_start_matches("0x")).expect("the state root is hex");

        let mut header = Vec::with_capacity(98);
        header.extend_from_slice(&[0u8; 32]); // parent hash
        header.push(0); // block number, SCALE compact 0
        header.extend_from_slice(&state_root);
        header.extend_from_slice(&hex::decode(EMPTY_TRIE_ROOT).expect("the constant is hex"));
        header.push(0); // digest, an empty vector

        blake2b_simd::Params::new()
            .hash_length(32)
            .hash(&header)
            .as_bytes()
            .try_into()
            .expect("a 32-byte hash")
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
