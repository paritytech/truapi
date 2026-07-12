use clap::ValueEnum;

/// Supported live network presets for the headless hosts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
    #[value(name = "paseo-next-v2")]
    #[default]
    PaseoNextV2,
}

impl Network {
    pub fn config(self) -> NetworkConfig {
        match self {
            Self::PaseoNextV2 => NetworkConfig {
                id: "paseo-next-v2",
                identity_backend_base: "https://identity-backend-next.parity-testnet.parity.io/api/v1",
                people_ws: "wss://paseo-people-next-system-rpc.polkadot.io",
                bulletin_ws: "wss://paseo-bulletin-next-rpc.polkadot.io",
                people_genesis: hex_literal_genesis(
                    "c5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5",
                ),
                bulletin_genesis: hex_literal_genesis(
                    "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
                ),
                live_chain_endpoints: PASEO_NEXT_V2_CHAIN_ENDPOINTS,
            },
        }
    }
}

const PASEO_NEXT_V2_CHAIN_ENDPOINTS: &[ChainEndpoint] = &[
    ChainEndpoint {
        genesis: hex_literal_genesis(
            "bf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f",
        ),
        ws: "wss://paseo-asset-hub-next-rpc.polkadot.io",
    },
    ChainEndpoint {
        genesis: hex_literal_genesis(
            "c5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5",
        ),
        ws: "wss://paseo-people-next-system-rpc.polkadot.io",
    },
    ChainEndpoint {
        genesis: hex_literal_genesis(
            "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
        ),
        ws: "wss://paseo-bulletin-next-rpc.polkadot.io",
    },
];

/// Resolved RPC/backend/genesis values for one network preset.
#[derive(Debug, Clone, Copy)]
pub struct NetworkConfig {
    pub id: &'static str,
    pub identity_backend_base: &'static str,
    pub people_ws: &'static str,
    #[allow(dead_code)]
    pub bulletin_ws: &'static str,
    pub people_genesis: [u8; 32],
    pub bulletin_genesis: [u8; 32],
    pub live_chain_endpoints: &'static [ChainEndpoint],
}

#[derive(Debug, Clone, Copy)]
pub struct ChainEndpoint {
    pub genesis: [u8; 32],
    pub ws: &'static str,
}

/// Decode a 64-char hex genesis at compile time.
const fn hex_literal_genesis(hex: &str) -> [u8; 32] {
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = hex_nibble(bytes[i * 2]) << 4 | hex_nibble(bytes[i * 2 + 1]);
        i += 1;
    }
    out
}

const fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("invalid hex digit in genesis literal"),
    }
}
