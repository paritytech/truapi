//! Headless TrUAPI hosts for local end-to-end testing.
//!
//! Three roles, one binary:
//! - `relay`: an in-memory statement-store the two hosts pair over.
//! - `pairing-host`: a seedless host that presents a pairing deeplink and
//!   serves product frames over WebSocket (the surface a product/test driver
//!   talks to).
//! - `signing-host`: a wallet-local host that answers a pairing deeplink and
//!   auto-signs, replacing the external signing-bot in e2e.

mod attestation;
mod chain;
mod frame_server;
mod platform;
mod relay;

use std::io::BufRead;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use futures::future::BoxFuture;
use truapi_platform::{HostInfo, PlatformInfo};
use truapi_server::subscription::Spawner;
use truapi_server::{PairingHostConfig, PairingHostRuntime, SigningHostConfig, SigningHostRuntime};

use crate::platform::{ApprovalPolicy, CliPlatform};

/// Default dev mnemonic used when a signing host is started without one.
const DEFAULT_MNEMONIC: &str =
    "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
/// Default product served by the pairing host's frame endpoint.
const DEFAULT_PRODUCT_ID: &str = "truapi-playground.dot";
/// Deeplink scheme advertised by the pairing host.
const DEEPLINK_SCHEME: &str = "polkadotapp";
/// paseo-next-v2 identity backend base (includes /api/v1).
const IDENTITY_BACKEND_BASE: &str = "https://identity-backend-next.parity-testnet.parity.io/api/v1";
/// paseo-next-v2 People-chain WebSocket for the attestation on-chain poll.
const PEOPLE_CHAIN_WS: &str = "wss://paseo-people-next-system-rpc.polkadot.io";
/// paseo-next-v2 People/Individuality chain genesis (username lookups).
const PEOPLE_CHAIN_GENESIS: [u8; 32] =
    hex_literal_genesis("c5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5");

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

#[derive(Parser)]
#[command(name = "truapi-host", about = "Headless TrUAPI hosts for e2e testing")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the in-memory statement-store relay.
    Relay {
        /// Address to listen on.
        #[arg(long, default_value = "127.0.0.1:9944")]
        listen: SocketAddr,
    },
    /// Run a seedless pairing host and serve product frames over WebSocket.
    PairingHost {
        /// Statement-store relay WebSocket URL.
        #[arg(long, default_value = "ws://127.0.0.1:9944")]
        relay: String,
        /// Address to serve product frames on.
        #[arg(long, default_value = "127.0.0.1:9955")]
        frame_listen: SocketAddr,
        /// Product id presented to product frame connections.
        #[arg(long, default_value = DEFAULT_PRODUCT_ID)]
        product: String,
        /// Resolve usernames from the real paseo-next-v2 People chain (so
        /// `get_user_id` works), instead of only the SSO relay.
        #[arg(long)]
        resolve_identity: bool,
    },
    /// Answer a pairing deeplink as a wallet-local signing host and auto-sign.
    SigningHost {
        /// Statement-store relay WebSocket URL.
        #[arg(long, default_value = "ws://127.0.0.1:9944")]
        relay: String,
        /// BIP-39 mnemonic for the wallet root.
        #[arg(long, default_value = DEFAULT_MNEMONIC)]
        mnemonic: String,
        /// Pairing deeplink to answer. Read from stdin when omitted.
        #[arg(long)]
        deeplink: Option<String>,
        /// Reject every sensitive action instead of auto-approving.
        #[arg(long)]
        reject: bool,
        /// Register this lite username base (6+ lowercase letters) on the
        /// People chain via the identity backend before pairing, so
        /// `get_user_id` resolves. Requires network access.
        #[arg(long)]
        username: Option<String>,
    },
    /// Probe the People chain for a mnemonic's registered identity/username.
    IdentityCheck {
        /// BIP-39 mnemonic to probe.
        #[arg(long, default_value = DEFAULT_MNEMONIC)]
        mnemonic: String,
        /// People-chain WebSocket URL.
        #[arg(long, default_value = PEOPLE_CHAIN_WS)]
        people_ws: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install a rustls crypto provider so `wss://` chain connections work;
    // rustls 0.23 panics without a process-level default provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Command::Relay { listen } => relay::Relay::serve(listen).await,
        Command::PairingHost {
            relay,
            frame_listen,
            product,
            resolve_identity,
        } => run_pairing_host(relay, frame_listen, product, resolve_identity).await,
        Command::SigningHost {
            relay,
            mnemonic,
            deeplink,
            reject,
            username,
        } => run_signing_host(relay, mnemonic, deeplink, reject, username).await,
        Command::IdentityCheck {
            mnemonic,
            people_ws,
        } => {
            let entropy = bip39::Mnemonic::parse(mnemonic.trim())
                .context("invalid BIP-39 mnemonic")?
                .to_entropy();
            attestation::check_identity(&people_ws, &entropy).await
        }
    }
}

/// Spawner that runs runtime futures on the tokio runtime, so their WebSocket
/// connects and timers have a reactor.
fn tokio_spawner() -> Spawner {
    Arc::new(|fut: BoxFuture<'static, ()>| {
        tokio::spawn(fut);
    })
}

fn host_info(name: &str) -> HostInfo {
    HostInfo {
        name: name.to_string(),
        icon: None,
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
    }
}

fn platform_info() -> PlatformInfo {
    PlatformInfo {
        kind: Some("cli".to_string()),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
    }
}

async fn run_pairing_host(
    relay: String,
    frame_listen: SocketAddr,
    product: String,
    resolve_identity: bool,
) -> Result<()> {
    let platform = CliPlatform::new(relay, ApprovalPolicy::Always);
    let mut config = PairingHostConfig::new(
        host_info("Headless Pairing Host"),
        platform_info(),
        [0u8; 32],
        DEEPLINK_SCHEME.to_string(),
    )
    .context("invalid pairing host config")?;
    if resolve_identity {
        // SSO stays on the relay ([0;32]); resolve usernames from the real
        // People chain. Requires live-chain routing (E2E_LIVE_CHAIN=1).
        config = config.with_identity_chain_genesis_hash(PEOPLE_CHAIN_GENESIS);
    }
    let runtime = Arc::new(PairingHostRuntime::new(platform, config, tokio_spawner()));
    frame_server::serve(runtime, product, frame_listen).await
}

async fn run_signing_host(
    relay: String,
    mnemonic: String,
    deeplink: Option<String>,
    reject: bool,
    username: Option<String>,
) -> Result<()> {
    let entropy = bip39::Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy();

    if let Some(username_base) = username {
        let registered = attestation::attest(&attestation::AttestConfig {
            backend_base: IDENTITY_BACKEND_BASE.to_string(),
            people_ws: PEOPLE_CHAIN_WS.to_string(),
            entropy: entropy.clone(),
            username_base,
        })
        .await
        .context("lite username attestation failed")?;
        println!("SIGNING_HOST_ATTESTED {registered}");
    }

    let deeplink = match deeplink {
        Some(deeplink) => deeplink,
        None => read_deeplink_from_stdin()?,
    };

    let approval = if reject {
        ApprovalPolicy::Never
    } else {
        ApprovalPolicy::Always
    };
    let platform = CliPlatform::new(relay, approval);
    let config = SigningHostConfig::new(
        host_info("Headless Signing Host"),
        platform_info(),
        [0u8; 32],
    )
    .context("invalid signing host config")?;
    let runtime = SigningHostRuntime::new(platform, config, tokio_spawner());
    runtime
        .activate_local_session(entropy)
        .await
        .map_err(|err| anyhow::anyhow!("failed to activate local session: {}", err.reason))?;
    println!("SIGNING_HOST_READY");

    let exit = runtime
        .respond_to_pairing(&deeplink)
        .await
        .map_err(|err| anyhow::anyhow!("pairing failed: {}", err.reason))?;
    println!("SIGNING_HOST_EXIT {exit:?}");
    Ok(())
}

fn read_deeplink_from_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.context("failed to read deeplink from stdin")?;
        let line = line.trim().to_string();
        if !line.is_empty() {
            return Ok(line);
        }
    }
    bail!("no pairing deeplink received on stdin");
}
