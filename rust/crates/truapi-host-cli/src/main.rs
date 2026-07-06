//! Headless TrUAPI hosts for local end-to-end testing.
//!
//! Two roles, one binary, pairing over the real People-chain statement store:
//! - `pairing-host`: a seedless host that presents a pairing deeplink and
//!   serves product frames over WebSocket (the surface a product/test driver
//!   talks to).
//! - `signing-host`: a wallet-local host that answers a pairing deeplink and
//!   auto-signs, replacing the external signing-bot in e2e.
//!
//! Plus `alloc-check`, a diagnostic for on-chain statement-store allowance.

mod alloc;
mod attestation;
mod chain;
mod frame_server;
mod platform;
mod script_runner;

use std::io::BufRead;
use std::net::SocketAddr;
use std::path::PathBuf;
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
/// Default product served by the pairing host's frame endpoint. Product ids
/// must be a `.dot` name or a `localhost` identifier (host-spec product id).
const DEFAULT_PRODUCT_ID: &str = "headless-playground.dot";
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
    /// Run a seedless pairing host and drive a product script against it.
    ///
    /// Starts the product-frame server, then runs `--script` with a global
    /// `truapi` injected (the `@parity/truapi` client, scoped to `--product-id`).
    /// The host command exits with the script's exit status.
    PairingHost {
        /// Product script to run (JS/TS). Receives the global `truapi`.
        #[arg(long)]
        script: PathBuf,
        /// Product id the host serves; scopes storage and product accounts.
        #[arg(long = "product-id", default_value = DEFAULT_PRODUCT_ID)]
        product_id: String,
        /// Address to serve product frames on.
        #[arg(long, default_value = "127.0.0.1:9955")]
        frame_listen: SocketAddr,
        /// Statement-store WebSocket URL (the real People chain by default).
        #[arg(long = "statement-store", default_value = PEOPLE_CHAIN_WS)]
        statement_store: String,
        /// Approve every confirmation without prompting on the CLI.
        #[arg(long)]
        auto_accept: bool,
    },
    /// Answer a pairing deeplink as a wallet-local signing host and sign.
    ///
    /// Registers statement allowance on-chain, answers the deeplink, and serves
    /// the SSO session. Confirmations are prompted on the CLI unless
    /// `--auto-accept` is set.
    SigningHost {
        /// Pairing deeplink to answer. Read from stdin when omitted.
        #[arg(long)]
        deeplink: Option<String>,
        /// BIP-39 mnemonic for the wallet root. Defaults to the
        /// `HOST_CLI_SIGNER_MNEMONIC` env var if set, otherwise the dev mnemonic.
        /// Must be a registered LitePeople ring member for allowance to succeed.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC", default_value = DEFAULT_MNEMONIC)]
        mnemonic: String,
        /// Statement-store WebSocket URL (the real People chain by default).
        #[arg(long = "statement-store", default_value = PEOPLE_CHAIN_WS)]
        statement_store: String,
        /// Approve every confirmation without prompting on the CLI.
        #[arg(long)]
        auto_accept: bool,
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
    /// Check (and optionally submit) a statement-store allowance registration
    /// against the real People chain: ring membership, the chosen slot, and
    /// (with `--submit`) the `set_statement_store_account` extrinsic.
    AllocCheck {
        /// BIP-39 mnemonic proving LitePeople ring membership.
        #[arg(long, default_value = DEFAULT_MNEMONIC)]
        mnemonic: String,
        /// People-chain WebSocket URL (statement store + chain RPC).
        #[arg(long, default_value = PEOPLE_CHAIN_WS)]
        people_ws: String,
        /// Target account (hex, 32 bytes) to grant allowance to. Defaults to
        /// all-zero (read-only slot scan only).
        #[arg(long)]
        target: Option<String>,
        /// How many rings back from the current index to scan for our member.
        #[arg(long, default_value_t = 8)]
        lookback: u32,
        /// Submit the extrinsic instead of only checking membership + slot.
        #[arg(long)]
        submit: bool,
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
        Command::PairingHost {
            script,
            product_id,
            frame_listen,
            statement_store,
            auto_accept,
        } => {
            run_pairing_host(
                script,
                product_id,
                frame_listen,
                statement_store,
                auto_accept,
            )
            .await
        }
        Command::SigningHost {
            deeplink,
            mnemonic,
            statement_store,
            auto_accept,
            username,
        } => run_signing_host(deeplink, mnemonic, statement_store, auto_accept, username).await,
        Command::IdentityCheck {
            mnemonic,
            people_ws,
        } => {
            let entropy = bip39::Mnemonic::parse(mnemonic.trim())
                .context("invalid BIP-39 mnemonic")?
                .to_entropy();
            attestation::check_identity(&people_ws, &entropy).await
        }
        Command::AllocCheck {
            mnemonic,
            people_ws,
            target,
            lookback,
            submit,
        } => run_alloc_check(mnemonic, people_ws, target, lookback, submit).await,
    }
}

/// Check statement-store allowance for a mnemonic: ring membership, the chosen
/// slot, and (with `submit`) the `set_statement_store_account` extrinsic.
async fn run_alloc_check(
    mnemonic: String,
    people_ws: String,
    target: Option<String>,
    lookback: u32,
    submit: bool,
) -> Result<()> {
    let entropy = bip39::Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy();
    let bandersnatch = alloc::bandersnatch_entropy(&entropy);

    let target = match target {
        Some(hex_str) => {
            let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(&hex_str))
                .context("invalid --target hex")?;
            <[u8; 32]>::try_from(bytes.as_slice())
                .map_err(|_| anyhow::anyhow!("--target must be 32 bytes"))?
        }
        None => [0u8; 32],
    };

    let rpc = alloc::rpc::RpcClient::connect(&people_ws).await?;
    let metadata = alloc::fetch_metadata(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let chain_state = alloc::fetch_chain_state(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    println!(
        "chain: specVersion={} txVersion={} genesis=0x{}",
        chain_state.spec_version,
        chain_state.transaction_version,
        hex::encode(chain_state.genesis_hash),
    );

    let member = alloc::proof::member_key(bandersnatch);
    println!("bandersnatch member=0x{}", hex::encode(member));
    let current_ring = alloc::ring::read_current_ring_index(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    println!("current ring index={current_ring}");
    let ring = alloc::find_including_ring(&rpc, &metadata, bandersnatch, lookback)
        .await
        .map_err(anyhow::Error::msg)?;
    match &ring {
        Some(r) => println!(
            "member INCLUDED in ring_index={} exponent={} included_members={}",
            r.ring_index,
            r.exponent,
            r.members.len(),
        ),
        None => println!("member NOT in the last {lookback} rings (onboarding pending)"),
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before UNIX epoch")?
        .as_secs();
    let period = alloc::slot::current_period(now);
    println!("period={period} target=0x{}", hex::encode(target));

    match alloc::slot::scan_slot(&rpc, &metadata, bandersnatch, period, &target).await {
        Ok(alloc::slot::SlotSelection::Free(seq)) => println!("slot scan: free seq={seq}"),
        Ok(alloc::slot::SlotSelection::AlreadyAllocated(seq)) => {
            println!("slot scan: target already allocated at seq={seq}")
        }
        Err(err) => println!("slot scan: {err}"),
    }

    if submit {
        let ring = ring.ok_or_else(|| anyhow::anyhow!("cannot submit: member not in any ring"))?;
        match alloc::register_statement_account(
            &rpc,
            &metadata,
            &chain_state,
            bandersnatch,
            &target,
            period,
            &ring,
        )
        .await
        {
            Ok(alloc::RegistrationOutcome::Registered {
                block_hash,
                seq,
                ring_index,
            }) => println!("REGISTERED seq={seq} ring_index={ring_index} block={block_hash}"),
            Ok(alloc::RegistrationOutcome::AlreadyAllocated { seq }) => {
                println!("already allocated at seq={seq}")
            }
            Err(err) => bail!("registration failed: {err}"),
        }
    }

    Ok(())
}

/// Map the `--auto-accept` flag to an approval policy: auto-accept, or prompt
/// each confirmation on the CLI.
fn approval_policy(auto_accept: bool) -> ApprovalPolicy {
    if auto_accept {
        ApprovalPolicy::AutoAccept
    } else {
        ApprovalPolicy::Prompt
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
    script: PathBuf,
    product_id: String,
    frame_listen: SocketAddr,
    statement_store: String,
    auto_accept: bool,
) -> Result<()> {
    let platform = CliPlatform::new(statement_store, approval_policy(auto_accept));
    // SSO and identity both run over the real People chain, so usernames always
    // resolve from `Resources.Consumers` (host-spec G).
    let config = PairingHostConfig::new(
        host_info("Headless Pairing Host"),
        platform_info(),
        [0u8; 32],
        DEEPLINK_SCHEME.to_string(),
    )
    .context("invalid pairing host config")?
    .with_identity_chain_genesis_hash(PEOPLE_CHAIN_GENESIS);
    let runtime = Arc::new(PairingHostRuntime::new(platform, config, tokio_spawner()));

    // Bind the frame server, then drive the product script against it; the
    // command exits with the script's status. The frame accept loop is `!Send`,
    // so it runs on a LocalSet alongside the (Send) script subprocess.
    let listener = frame_server::bind(frame_listen).await?;
    let frame_url = format!("ws://{}", listener.local_addr()?);
    println!("FRAMES_LISTENING {frame_url}");

    let local = tokio::task::LocalSet::new();
    let status = local
        .run_until(async move {
            let server = tokio::task::spawn_local(frame_server::accept_loop(
                runtime,
                product_id.clone(),
                listener,
            ));
            let status = script_runner::run(&frame_url, &product_id, &script).await;
            server.abort();
            status
        })
        .await?;

    std::process::exit(status.code().unwrap_or(1));
}

async fn run_signing_host(
    deeplink: Option<String>,
    mnemonic: String,
    statement_store: String,
    auto_accept: bool,
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

    // Grant statement-store allowance to the accounts that submit statements
    // over the real store: our own `//wallet//sso` and the pairing host's
    // device key. A real client does this on-chain; without it the store
    // rejects the handshake with `NoAllowance`.
    register_pairing_allowances(&statement_store, &entropy, &deeplink).await?;

    let platform = CliPlatform::new(statement_store, approval_policy(auto_accept));
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

/// Grant on-chain statement-store allowance to the two accounts that submit
/// statements during pairing: the signing host's own `//wallet//sso` account
/// and the pairing host's per-pairing device key (from the deeplink). Proves
/// the signing account's LitePeople ring membership once and reuses it.
async fn register_pairing_allowances(
    statement_store_url: &str,
    entropy: &[u8],
    deeplink: &str,
) -> Result<()> {
    use truapi_server::host_logic::product_account::derive_sr25519_hard_path;
    use truapi_server::host_logic::sso::pairing::{
        VersionedHandshakeProposal, decode_pairing_deeplink,
    };

    let wallet_sso = derive_sr25519_hard_path(entropy, &["wallet", "sso"])
        .map_err(|e| anyhow::anyhow!("//wallet//sso derivation failed: {e}"))?
        .public
        .to_bytes();
    let VersionedHandshakeProposal::V2(proposal) =
        decode_pairing_deeplink(deeplink).map_err(anyhow::Error::msg)?;
    let device = proposal.device.statement_account_id;

    let bandersnatch = alloc::bandersnatch_entropy(entropy);
    let rpc = alloc::rpc::RpcClient::connect(statement_store_url).await?;
    let metadata = alloc::fetch_metadata(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let chain_state = alloc::fetch_chain_state(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;

    // The signing account may be in an old ring, so scan back to genesis.
    let current = alloc::ring::read_current_ring_index(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let ring = alloc::find_including_ring(&rpc, &metadata, bandersnatch, current)
        .await
        .map_err(anyhow::Error::msg)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "signing account is not a LitePeople ring member; cannot grant allowance"
            )
        })?;
    println!(
        "SIGNING_HOST_RING ring_index={} members={}",
        ring.ring_index,
        ring.members.len()
    );

    let period = alloc::slot::current_period(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock before UNIX epoch")?
            .as_secs(),
    );

    for (label, target) in [("wallet-sso", wallet_sso), ("device", device)] {
        let outcome = alloc::register_statement_account(
            &rpc,
            &metadata,
            &chain_state,
            bandersnatch,
            &target,
            period,
            &ring,
        )
        .await
        .map_err(|e| anyhow::anyhow!("allowance registration for {label} failed: {e}"))?;
        match outcome {
            alloc::RegistrationOutcome::Registered {
                block_hash, seq, ..
            } => println!("SIGNING_HOST_ALLOWANCE {label} seq={seq} block={block_hash}"),
            alloc::RegistrationOutcome::AlreadyAllocated { seq } => {
                println!("SIGNING_HOST_ALLOWANCE {label} already-allocated seq={seq}")
            }
        }
    }
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
