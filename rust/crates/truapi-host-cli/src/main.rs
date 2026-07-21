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

mod accounts;
mod attestation;
mod chain;
mod frame_server;
mod network;
mod platform;
mod script_runner;
mod signing_shell;
mod terminal_ui;

use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures::future::BoxFuture;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use truapi_platform::{HostInfo, PlatformInfo};
use truapi_server::statement_allowance as alloc;
use truapi_server::subscription::Spawner;
use truapi_server::{PairingHostConfig, PairingHostRuntime, SigningHostConfig, SigningHostRuntime};

use crate::accounts::{ResolveSignerConfig, ResolvedSigner};
use crate::network::{Network, NetworkConfig};
use crate::platform::{ApprovalPolicy, CliPlatform};
use crate::signing_shell::{HELP_TEXT, ShellCommand, parse_command};
use crate::terminal_ui::{ActiveTerminalUi, DriveResult, TerminalUi, UiHandle};

/// Default product served by the pairing host's frame endpoint. Product ids
/// must be a `.dot` name or a `localhost` identifier (host-spec product id).
const DEFAULT_PRODUCT_ID: &str = "headless-playground.dot";
/// Default product-frame address for the pairing host.
const DEFAULT_PAIRING_FRAME_LISTEN: &str = "127.0.0.1:9955";
/// Default product-frame address for the signing host.
const DEFAULT_SIGNING_FRAME_LISTEN: &str = "127.0.0.1:9956";
/// Deeplink scheme advertised by the pairing host.
const DEEPLINK_SCHEME: &str = "polkadotapp";

#[derive(Parser)]
#[command(name = "truapi-host", about = "Headless TrUAPI hosts for e2e testing")]
struct Cli {
    /// Log verbosity. `RUST_LOG` takes precedence when set.
    #[arg(
        long,
        global = true,
        value_enum,
        env = "TRUAPI_HOST_LOG",
        default_value = "info"
    )]
    log_level: LogLevel,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    const fn as_filter(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(format!(
                "invalid log level `{value}`; expected error, warn, info, debug, or trace"
            )),
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_filter())
    }
}

#[derive(Clone)]
struct LogController {
    reload: Arc<dyn Fn(LogLevel) -> Result<(), String> + Send + Sync>,
}

impl LogController {
    fn set(&self, level: LogLevel) -> Result<()> {
        (self.reload)(level).map_err(anyhow::Error::msg)
    }
}

#[derive(Subcommand)]
enum Command {
    /// Run a seedless pairing host for product scripts or interactive pairing.
    ///
    /// With `--script`, exits with the script's status. Without it, stays in an
    /// interactive shell where scripts can be run repeatedly.
    PairingHost(PairingHostArgs),
    /// Run a wallet-local signing host for scripts or pairing deeplinks.
    ///
    /// Owns signer identity, auto-manages accounts when no mnemonic/account is
    /// specified, and can accept pairing deeplinks. With `--script`, exits with
    /// the script's status; otherwise stays interactive.
    SigningHost(SigningHostArgs),
    /// Probe the People chain for a mnemonic's registered identity/username.
    IdentityCheck {
        /// BIP-39 mnemonic to probe.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
        mnemonic: String,
        /// Network preset to probe.
        #[arg(long, value_enum, default_value = "paseo-next-v2")]
        network: Network,
    },
    /// Check (and optionally submit) a statement-store allowance registration
    /// against the real People chain: ring membership, the chosen slot, and
    /// (with `--submit`) the `set_statement_store_account` extrinsic.
    AllocCheck {
        /// BIP-39 mnemonic proving LitePeople ring membership.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
        mnemonic: String,
        /// Network preset to use for People-chain RPC.
        #[arg(long, value_enum, default_value = "paseo-next-v2")]
        network: Network,
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

#[derive(Args)]
struct PairingHostArgs {
    /// Product script to run (JS/TS). If omitted, start the terminal UI.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Product id the host serves; scopes storage and product accounts.
    #[arg(long = "product-id", default_value = DEFAULT_PRODUCT_ID)]
    product_id: String,
    /// Address to serve product frames on.
    #[arg(long, default_value = DEFAULT_PAIRING_FRAME_LISTEN)]
    frame_listen: SocketAddr,
    /// Root directory for CLI-managed host state.
    #[arg(long = "base-path", env = "TRUAPI_HOST_BASE_PATH")]
    base_path: Option<PathBuf>,
    /// Network preset that supplies all RPC/backend/genesis config.
    #[arg(long, value_enum, default_value = "paseo-next-v2")]
    network: Network,
    /// Approve every confirmation without prompting on the CLI.
    #[arg(long)]
    auto_accept: bool,
}

#[derive(Args)]
struct SigningHostArgs {
    /// Product script to run (JS/TS). If omitted, start an interactive shell.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Product id used by scripts and product-scoped operations.
    #[arg(long = "product-id", default_value = DEFAULT_PRODUCT_ID)]
    product_id: String,
    /// Pairing deeplink to answer. If omitted, no pairing is accepted
    /// automatically; interactive mode lets you paste one later.
    #[arg(long)]
    deeplink: Option<String>,
    /// BIP-39 mnemonic for the wallet root. If omitted, the
    /// `HOST_CLI_SIGNER_MNEMONIC` env var is used when set. Any mnemonic
    /// bypasses account auto-management.
    #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
    mnemonic: Option<String>,
    /// Named stored account to use. Omit this and `--mnemonic` to auto-select
    /// or create a usable account.
    #[arg(long)]
    account: Option<String>,
    /// Prefix for newly-created lite usernames in auto-account mode.
    #[arg(long = "lite-username-prefix")]
    lite_username_prefix: Option<String>,
    /// Root directory for CLI-managed account and host state.
    #[arg(long = "base-path", env = "TRUAPI_HOST_BASE_PATH")]
    base_path: Option<PathBuf>,
    /// Network preset that supplies all RPC/backend/genesis config.
    #[arg(long, value_enum, default_value = "paseo-next-v2")]
    network: Network,
    /// Address to serve product frames on when running scripts.
    #[arg(long, default_value = DEFAULT_SIGNING_FRAME_LISTEN)]
    frame_listen: SocketAddr,
    /// Approve every confirmation without prompting on the CLI.
    #[arg(long)]
    auto_accept: bool,
    /// Execute one slash command without starting the terminal UI.
    #[command(subcommand)]
    action: Option<SigningHostAction>,
}

#[derive(Subcommand)]
enum SigningHostAction {
    /// Execute one slash command and exit.
    Exec {
        /// Slash command to execute, such as `/whoami`.
        command: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install a rustls crypto provider so `wss://` chain connections work;
    // rustls 0.23 panics without a process-level default provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(cli.log_level.as_filter()));
    let (filter, reload) = tracing_subscriber::reload::Layer::new(filter);
    let log_controller = LogController {
        reload: Arc::new(move |level| {
            reload
                .reload(tracing_subscriber::EnvFilter::new(level.as_filter()))
                .map_err(|error| error.to_string())
        }),
    };
    let log_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(terminal_ui::LogWriter::default)
        .with_filter(filter)
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.target() != terminal_ui::INCOMING_SSO_TARGET
        }));
    tracing_subscriber::registry()
        .with(terminal_ui::IncomingSsoLayer)
        .with(log_layer)
        .init();

    match cli.command {
        Command::PairingHost(args) => run_pairing_host(args).await,
        Command::SigningHost(args) => run_signing_host(args, cli.log_level, log_controller).await,
        Command::IdentityCheck { mnemonic, network } => {
            let entropy = bip39::Mnemonic::parse(mnemonic.trim())
                .context("invalid BIP-39 mnemonic")?
                .to_entropy();
            attestation::check_identity(network.config().people_ws, &entropy).await
        }
        Command::AllocCheck {
            mnemonic,
            network,
            target,
            lookback,
            submit,
        } => run_alloc_check(mnemonic, network.config(), target, lookback, submit).await,
    }
}

/// Check statement-store allowance for a mnemonic: ring membership, the chosen
/// slot, and (with `submit`) the `set_statement_store_account` extrinsic.
async fn run_alloc_check(
    mnemonic: String,
    network: NetworkConfig,
    target: Option<String>,
    lookback: u32,
    submit: bool,
) -> Result<()> {
    let entropy = bip39::Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy();
    let bandersnatch = alloc::bandersnatch_entropy(&entropy);

    if submit && target.is_none() {
        bail!("--target is required with --submit; the all-zero default is read-only");
    }

    let target = match target {
        Some(hex_str) => {
            let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(&hex_str))
                .context("invalid --target hex")?;
            <[u8; 32]>::try_from(bytes.as_slice())
                .map_err(|_| anyhow::anyhow!("--target must be 32 bytes"))?
        }
        None => [0u8; 32],
    };

    let rpc = alloc::rpc::RpcClient::connect(network.people_ws)
        .await
        .map_err(anyhow::Error::msg)?;
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

    match alloc::slot::scan_slot_excluding(&rpc, &metadata, bandersnatch, period, &target, &[])
        .await
    {
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

async fn run_pairing_host(args: PairingHostArgs) -> Result<()> {
    let network = args.network.config();
    let base_path = args.base_path.unwrap_or_else(default_base_path);
    let product_id = args.product_id;
    let platform = CliPlatform::new(
        network.people_ws,
        network.live_chain_endpoints,
        Some(role_state_path(&base_path, network, "pairing-host")),
        approval_policy(args.auto_accept),
        None,
    );
    // SSO and identity both run over the real People chain, so usernames always
    // resolve from `Resources.Consumers` (host-spec G).
    let config = PairingHostConfig::new(
        host_info("Headless Pairing Host"),
        platform_info(),
        network.people_genesis,
        network.bulletin_genesis,
        DEEPLINK_SCHEME.to_string(),
    )
    .context("invalid pairing host config")?;
    let runtime = Arc::new(PairingHostRuntime::new(platform, config, tokio_spawner()));

    let listener = frame_server::bind(args.frame_listen).await?;
    let frame_url = format!("ws://{}", listener.local_addr()?);
    println!("FRAMES_LISTENING {frame_url}");
    let runtime: Arc<dyn frame_server::ProductRuntimeFactory> = runtime;

    if let Some(script) = args.script {
        let script_product_id = product_id.clone();
        let script_frame_url = frame_url.clone();
        let status = with_frame_server(runtime, product_id, listener, async move {
            script_runner::run(&script_frame_url, &script_product_id, &script).await
        })
        .await?;
        std::process::exit(status.code().unwrap_or(1));
    }

    with_frame_server(runtime, product_id.clone(), listener, async move {
        pairing_interactive_loop(frame_url, product_id).await
    })
    .await
}

async fn run_signing_host(
    args: SigningHostArgs,
    initial_log_level: LogLevel,
    log_controller: LogController,
) -> Result<()> {
    if let Err(error) = validate_signing_args(&args) {
        invalid_invocation(error);
    }
    let exec_input = args
        .action
        .as_ref()
        .map(|SigningHostAction::Exec { command }| command.clone());
    let interactive = args.script.is_none() && exec_input.is_none();
    if interactive && !terminal_ui::is_interactive_terminal() {
        invalid_invocation(
            "interactive signing-host requires a TTY; use `signing-host exec '/whoami'` or --script",
        );
    }
    let exec_command = exec_input
        .as_deref()
        .map(|input| parse_command(input).unwrap_or_else(|error| invalid_invocation(error)));
    let network = args.network.config();
    let base_path = args.base_path.clone().unwrap_or_else(default_base_path);
    let (terminal_ui, ui_handle) = if interactive {
        let (ui, handle) = TerminalUi::new(network.id, initial_log_level);
        (Some(ui.enter()?), Some(handle))
    } else {
        (None, None)
    };
    let mut session = start_signing_host(&args, base_path, network, ui_handle.clone()).await?;
    let listener = frame_server::bind(args.frame_listen).await?;
    let frame_url = format!("ws://{}", listener.local_addr()?);
    if let Some(ui) = &ui_handle {
        ui.system(format!("FRAMES_LISTENING {frame_url}"));
        ui.system("Type /help to list commands");
    } else {
        println!("FRAMES_LISTENING {frame_url}");
    }
    let runtime_for_frames: Arc<dyn frame_server::ProductRuntimeFactory> = session.runtime.clone();

    if let Some(script) = args.script {
        let product_id = args.product_id.clone();
        let script_product_id = product_id.clone();
        let script_frame_url = frame_url.clone();
        let initial_deeplink = args.deeplink.clone();
        let status = with_frame_server(runtime_for_frames, product_id, listener, async move {
            let mut responder = None;
            if let Some(deeplink) = initial_deeplink {
                prepare_pairing_response(&mut session, &deeplink).await?;
                let runtime = session.runtime.clone();
                responder = Some(tokio::spawn(async move {
                    match runtime.respond_to_pairing(&deeplink).await {
                        Ok(exit) => println!("SIGNING_HOST_EXIT {exit:?}"),
                        Err(err) => eprintln!("SIGNING_HOST_ERROR {}", err.reason),
                    }
                }));
            }
            ensure_signer(&mut session).await?;
            let status = script_runner::run(&script_frame_url, &script_product_id, &script).await?;
            if let Some(responder) = responder {
                responder.abort();
            }
            Ok::<ExitStatus, anyhow::Error>(status)
        })
        .await?;
        std::process::exit(status.code().unwrap_or(1));
    }

    let product_id = args.product_id.clone();
    let initial_deeplink = args.deeplink.clone();
    if let Some(command) = exec_command {
        return with_frame_server(
            runtime_for_frames,
            product_id.clone(),
            listener,
            async move {
                let responder = if let Some(deeplink) = initial_deeplink {
                    prepare_pairing_response(&mut session, &deeplink).await?;
                    let runtime = session.runtime.clone();
                    Some(tokio::spawn(async move {
                        match runtime.respond_to_pairing(&deeplink).await {
                            Ok(exit) => println!("SIGNING_HOST_EXIT {exit:?}"),
                            Err(err) => eprintln!("SIGNING_HOST_ERROR {}", err.reason),
                        }
                    }))
                } else {
                    None
                };
                let result = execute_non_interactive_command(
                    &mut session,
                    &frame_url,
                    &product_id,
                    command,
                    &log_controller,
                )
                .await;
                if let Some(responder) = responder {
                    responder.abort();
                }
                result
            },
        )
        .await;
    }

    let terminal_ui = terminal_ui.context("interactive terminal was not initialized")?;
    with_frame_server(
        runtime_for_frames,
        product_id.clone(),
        listener,
        async move {
            signing_interactive_loop(
                &mut session,
                frame_url,
                product_id,
                initial_deeplink,
                terminal_ui,
                log_controller,
            )
            .await
        },
    )
    .await
}

struct SigningHostSession {
    runtime: Arc<SigningHostRuntime>,
    responder: Option<tokio::task::JoinHandle<()>>,
    signer: Option<ResolvedSigner>,
    base_path: PathBuf,
    network: NetworkConfig,
    mnemonic: Option<String>,
    account: Option<String>,
    lite_username_prefix: Option<String>,
}

async fn start_signing_host(
    args: &SigningHostArgs,
    base_path: PathBuf,
    network: NetworkConfig,
    ui: Option<UiHandle>,
) -> Result<SigningHostSession> {
    let platform = CliPlatform::new(
        network.people_ws,
        network.live_chain_endpoints,
        Some(role_state_path(&base_path, network, "signing-host")),
        approval_policy(args.auto_accept),
        ui,
    );
    let config = SigningHostConfig::new(
        host_info("Headless Signing Host"),
        platform_info(),
        network.people_genesis,
        network.bulletin_genesis,
    )
    .context("invalid signing host config")?;
    let runtime = Arc::new(SigningHostRuntime::new(platform, config, tokio_spawner()));

    Ok(SigningHostSession {
        runtime,
        responder: None,
        signer: None,
        base_path,
        network,
        mnemonic: normalized(args.mnemonic.clone()),
        account: normalized(args.account.clone()),
        lite_username_prefix: normalized(args.lite_username_prefix.clone()),
    })
}

impl Drop for SigningHostSession {
    fn drop(&mut self) {
        if let Some(responder) = self.responder.take() {
            responder.abort();
        }
    }
}

fn validate_signing_args(args: &SigningHostArgs) -> Result<()> {
    let mnemonic = normalized(args.mnemonic.clone());
    let account = normalized(args.account.clone());
    let prefix = normalized(args.lite_username_prefix.clone());
    if args.script.is_some() && args.action.is_some() {
        bail!("--script cannot be combined with the exec subcommand");
    }
    if mnemonic.is_some() && account.is_some() {
        bail!("--account cannot be used when --mnemonic or HOST_CLI_SIGNER_MNEMONIC is set");
    }
    if mnemonic.is_some() && prefix.is_some() {
        bail!(
            "--lite-username-prefix cannot be used when --mnemonic or HOST_CLI_SIGNER_MNEMONIC is set"
        );
    }
    if account.is_some() && prefix.is_some() {
        bail!("--lite-username-prefix only applies when --account is omitted");
    }
    Ok(())
}

fn normalized(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn invalid_invocation(error: impl fmt::Display) -> ! {
    eprintln!("error: {error}");
    std::process::exit(2);
}

async fn with_frame_server<T, Fut>(
    runtime: Arc<dyn frame_server::ProductRuntimeFactory>,
    product_id: String,
    listener: tokio::net::TcpListener,
    body: Fut,
) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let server =
                tokio::task::spawn_local(frame_server::accept_loop(runtime, product_id, listener));
            let result = body.await;
            server.abort();
            result
        })
        .await
}

async fn ensure_signer(session: &mut SigningHostSession) -> Result<()> {
    if session.signer.is_some() {
        return Ok(());
    }
    session.signer = Some(
        accounts::resolve_signer(ResolveSignerConfig {
            base_path: &session.base_path,
            network: session.network,
            mnemonic: session.mnemonic.clone(),
            account: session.account.clone(),
            lite_username_prefix: session.lite_username_prefix.clone(),
        })
        .await?,
    );
    activate_current_signer(session).await
}

async fn activate_current_signer(session: &mut SigningHostSession) -> Result<()> {
    let signer = session
        .signer
        .as_ref()
        .context("signer has not been resolved")?;
    session
        .runtime
        .activate_local_session_with_identity(signer.entropy.clone(), signer.lite_username.clone())
        .await
        .map_err(|err| anyhow::anyhow!("failed to activate local session: {}", err.reason))?;
    terminal_ui::output_line("SIGNING_HOST_READY");
    Ok(())
}

async fn prepare_pairing_response(session: &mut SigningHostSession, deeplink: &str) -> Result<()> {
    let mut attempts = 0usize;
    loop {
        ensure_signer(session).await?;
        let (entropy, auto_managed, account_name) = {
            let signer = session
                .signer
                .as_ref()
                .context("signer has not been resolved")?;
            (
                signer.entropy.clone(),
                signer.auto_managed,
                signer.account_name.clone(),
            )
        };
        match register_pairing_allowances(session.network.people_ws, &entropy, deeplink).await {
            Ok(()) => return Ok(()),
            Err(err) if auto_managed && is_statement_slot_exhaustion(&err) => {
                attempts += 1;
                if attempts > 8 {
                    return Err(err);
                }
                if let Some(name) = &account_name {
                    let period = accounts::current_statement_period()?;
                    accounts::mark_account_exhausted(
                        &session.base_path,
                        session.network.id,
                        name,
                        period,
                    )?;
                    terminal_ui::output_line(format!(
                        "SIGNING_HOST_ACCOUNT_EXHAUSTED {name} period={period}"
                    ));
                }
                session.signer = Some(
                    accounts::resolve_signer(ResolveSignerConfig {
                        base_path: &session.base_path,
                        network: session.network,
                        mnemonic: None,
                        account: None,
                        lite_username_prefix: session.lite_username_prefix.clone(),
                    })
                    .await?,
                );
                activate_current_signer(session).await?;
            }
            Err(err) => return Err(err),
        }
    }
}

fn is_statement_slot_exhaustion(err: &anyhow::Error) -> bool {
    err.to_string().contains("no free StatementStore slot")
}

async fn respond_to_deeplink(session: &mut SigningHostSession, deeplink: String) -> Result<()> {
    prepare_pairing_response(session, &deeplink).await?;
    let exit = session
        .runtime
        .respond_to_pairing(&deeplink)
        .await
        .map_err(|err| anyhow::anyhow!("pairing failed: {}", err.reason))?;
    terminal_ui::output_line(format!("SIGNING_HOST_EXIT {exit:?}"));
    Ok(())
}

async fn start_deeplink_responder(
    session: &mut SigningHostSession,
    deeplink: String,
) -> Result<()> {
    prepare_pairing_response(session, &deeplink).await?;
    if let Some(responder) = session.responder.take() {
        responder.abort();
    }
    let runtime = session.runtime.clone();
    session.responder = Some(tokio::spawn(async move {
        match runtime.respond_to_pairing(&deeplink).await {
            Ok(exit) => terminal_ui::output_line(format!("SIGNING_HOST_EXIT {exit:?}")),
            Err(error) => {
                terminal_ui::output_line(format!("SIGNING_HOST_ERROR {}", error.reason));
            }
        }
    }));
    terminal_ui::output_line("SIGNING_HOST_RESPONDER_STARTED");
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
    let rpc = alloc::rpc::RpcClient::connect(statement_store_url)
        .await
        .map_err(anyhow::Error::msg)?;
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
    terminal_ui::output_line(format!(
        "SIGNING_HOST_RING ring_index={} members={}",
        ring.ring_index,
        ring.members.len()
    ));

    let period = alloc::slot::current_period(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock before UNIX epoch")?
            .as_secs(),
    );

    for (label, target) in [("wallet-sso", wallet_sso), ("device", device)] {
        terminal_ui::output_line(format!("SIGNING_HOST_ALLOWANCE {label} checking"));
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
            } => terminal_ui::output_line(format!(
                "SIGNING_HOST_ALLOWANCE {label} seq={seq} block={block_hash}"
            )),
            alloc::RegistrationOutcome::AlreadyAllocated { seq } => {
                terminal_ui::output_line(format!(
                    "SIGNING_HOST_ALLOWANCE {label} already-allocated seq={seq}"
                ));
            }
        }
    }
    Ok(())
}

async fn pairing_interactive_loop(frame_url: String, product_id: String) -> Result<()> {
    println!("PAIRING_HOST_INTERACTIVE commands: script <path>, quit");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print_prompt("pairing-host> ").await?;
        let Some(line) = lines.next_line().await? else {
            return Ok(());
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if is_quit(line) {
            return Ok(());
        }
        let Some(script) = script_command(line) else {
            println!("unknown command; use: script <path>, quit");
            continue;
        };
        let status = script_runner::run(&frame_url, &product_id, &script).await?;
        println!("SCRIPT_EXIT {}", status.code().unwrap_or(1));
    }
}

async fn signing_interactive_loop(
    session: &mut SigningHostSession,
    frame_url: String,
    product_id: String,
    initial_deeplink: Option<String>,
    mut ui: ActiveTerminalUi,
    log_controller: LogController,
) -> Result<()> {
    if let Some(deeplink) = initial_deeplink {
        let input = format!("/deeplink {deeplink}");
        ui.command(input.clone());
        run_interactive_operation(
            session,
            &frame_url,
            &product_id,
            ShellCommand::Deeplink(deeplink),
            input,
            &mut ui,
        )
        .await?;
    }

    loop {
        let Some(input) = ui.next_command().await? else {
            return Ok(());
        };
        ui.command(input.clone());
        let command = match parse_command(&input) {
            Ok(command) => command,
            Err(error) => {
                ui.error(error);
                continue;
            }
        };
        match command {
            ShellCommand::Help => ui.system(HELP_TEXT),
            ShellCommand::Clear => ui.clear(),
            ShellCommand::Log(level) => {
                if let Err(error) = log_controller.set(level) {
                    ui.error(format!("failed to set log level: {error}"));
                } else {
                    ui.set_log_level(level);
                    ui.system(format!("LOG_LEVEL {level}"));
                }
            }
            ShellCommand::Quit => return Ok(()),
            command => {
                run_interactive_operation(
                    session,
                    &frame_url,
                    &product_id,
                    command,
                    input,
                    &mut ui,
                )
                .await?;
            }
        }
    }
}

async fn run_interactive_operation(
    session: &mut SigningHostSession,
    frame_url: &str,
    product_id: &str,
    command: ShellCommand,
    label: String,
    ui: &mut ActiveTerminalUi,
) -> Result<()> {
    let handle = ui.handle();
    let operation = execute_interactive_operation(session, frame_url, product_id, command, handle);
    match ui.drive(label, operation).await? {
        DriveResult::Complete(Ok(())) => {}
        DriveResult::Complete(Err(error)) => ui.error(error.to_string()),
        DriveResult::Cancelled => ui.error("command cancelled"),
    }
    Ok(())
}

async fn execute_interactive_operation(
    session: &mut SigningHostSession,
    frame_url: &str,
    product_id: &str,
    command: ShellCommand,
    ui: UiHandle,
) -> Result<()> {
    match command {
        ShellCommand::Whoami => {
            ensure_signer(session).await?;
            let script = script_runner::bundled_script("whoami.ts");
            let status = script_runner::run_captured(frame_url, product_id, &script, ui).await?;
            if !status.success() {
                bail!("WHOAMI_EXIT {}", status.code().unwrap_or(1));
            }
        }
        ShellCommand::Deeplink(deeplink) => start_deeplink_responder(session, deeplink).await?,
        ShellCommand::Script(script) => {
            ensure_signer(session).await?;
            let status = script_runner::run_captured(frame_url, product_id, &script, ui).await?;
            if !status.success() {
                bail!("SCRIPT_EXIT {}", status.code().unwrap_or(1));
            }
            terminal_ui::output_line("SCRIPT_EXIT 0");
        }
        ShellCommand::Help | ShellCommand::Clear | ShellCommand::Log(_) | ShellCommand::Quit => {
            bail!("command must be handled by the terminal UI")
        }
    }
    Ok(())
}

async fn execute_non_interactive_command(
    session: &mut SigningHostSession,
    frame_url: &str,
    product_id: &str,
    command: ShellCommand,
    log_controller: &LogController,
) -> Result<()> {
    match command {
        ShellCommand::Whoami => {
            ensure_signer(session).await?;
            let script = script_runner::bundled_script("whoami.ts");
            let status = script_runner::run(frame_url, product_id, &script).await?;
            if !status.success() {
                bail!("WHOAMI_EXIT {}", status.code().unwrap_or(1));
            }
        }
        ShellCommand::Deeplink(deeplink) => respond_to_deeplink(session, deeplink).await?,
        ShellCommand::Script(script) => {
            ensure_signer(session).await?;
            let status = script_runner::run(frame_url, product_id, &script).await?;
            if !status.success() {
                bail!("SCRIPT_EXIT {}", status.code().unwrap_or(1));
            }
            println!("SCRIPT_EXIT 0");
        }
        ShellCommand::Help => println!("{HELP_TEXT}"),
        ShellCommand::Clear | ShellCommand::Quit => {}
        ShellCommand::Log(level) => {
            log_controller.set(level)?;
            println!("LOG_LEVEL {level}");
        }
    }
    Ok(())
}

async fn print_prompt(prompt: &str) -> Result<()> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(prompt.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

fn is_quit(line: &str) -> bool {
    matches!(line, "quit" | "exit" | "q")
}

fn script_command(line: &str) -> Option<PathBuf> {
    line.strip_prefix("script ")
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn default_base_path() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path).join("truapi-host");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/truapi-host");
    }
    PathBuf::from(".truapi-host")
}

fn role_state_path(base_path: &std::path::Path, network: NetworkConfig, role: &str) -> PathBuf {
    base_path.join(network.id).join(role)
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn trace_log_level_is_available_before_or_after_the_subcommand() {
        let before = Cli::try_parse_from(["truapi-host", "--log-level", "trace", "signing-host"])
            .expect("global log level before subcommand should parse");
        let after = Cli::try_parse_from(["truapi-host", "signing-host", "--log-level", "trace"])
            .expect("global log level after subcommand should parse");

        assert_eq!(before.log_level, LogLevel::Trace);
        assert_eq!(after.log_level, LogLevel::Trace);
        assert_eq!(LogLevel::Trace.as_filter(), "trace");
    }

    #[test]
    fn signing_host_exec_accepts_one_slash_command() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--auto-accept",
            "exec",
            "/whoami",
        ])
        .expect("exec slash command should parse");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };
        let Some(SigningHostAction::Exec { command }) = args.action else {
            panic!("expected exec action");
        };
        assert_eq!(command, "/whoami");
    }

    #[test]
    fn signing_host_rejects_script_and_exec_together() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--script",
            "smoke.ts",
            "exec",
            "/whoami",
        ])
        .expect("clap should parse parent options before exec");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };
        assert!(
            validate_signing_args(&args)
                .unwrap_err()
                .to_string()
                .contains("--script cannot be combined")
        );
    }
}
