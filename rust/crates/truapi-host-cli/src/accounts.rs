use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use bip39::Mnemonic;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use truapi_server::host_logic::product_account::{
    derive_sr25519_hard_path, product_public_key_to_address,
};

use crate::alloc;
use crate::attestation;
use crate::network::NetworkConfig;

const ACCOUNT_STORE_FILE: &str = "accounts.json";
const DEFAULT_USERNAME_PREFIX: &str = "headless";

#[derive(Debug, Clone)]
pub struct ResolvedSigner {
    pub entropy: Vec<u8>,
    pub account_name: Option<String>,
    pub lite_username: Option<String>,
    pub auto_managed: bool,
}

#[derive(Debug, Clone)]
pub struct ResolveSignerConfig<'a> {
    pub base_path: &'a Path,
    pub network: NetworkConfig,
    pub mnemonic: Option<String>,
    pub account: Option<String>,
    pub lite_username_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    pub name: String,
    pub network: String,
    pub mnemonic: String,
    pub lite_username: String,
    pub public_key_hex: String,
    pub address: String,
    pub created_at_unix: u64,
    #[serde(default)]
    pub attested: bool,
    #[serde(default)]
    exhausted_statement_periods: BTreeSet<u32>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountStoreData {
    version: u32,
    accounts: Vec<AccountRecord>,
}

pub struct AccountStore {
    path: PathBuf,
    data: AccountStoreData,
}

impl AccountStore {
    pub fn load(base_path: &Path) -> Result<Self> {
        let path = base_path.join(ACCOUNT_STORE_FILE);
        let data = match fs::read_to_string(&path) {
            Ok(text) => {
                serde_json::from_str(&text).with_context(|| format!("decode {}", path.display()))?
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => AccountStoreData {
                version: 1,
                accounts: Vec::new(),
            },
            Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
        };
        Ok(Self { path, data })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(&self.data)?;
        write_secret_file(&self.path, text.as_bytes())
            .with_context(|| format!("write {}", self.path.display()))
    }

    pub fn get(&self, network_id: &str, name: &str) -> Option<&AccountRecord> {
        self.data
            .accounts
            .iter()
            .find(|record| record.network == network_id && record.name == name)
    }

    fn upsert(&mut self, record: AccountRecord) {
        if let Some(existing) = self
            .data
            .accounts
            .iter_mut()
            .find(|existing| existing.network == record.network && existing.name == record.name)
        {
            *existing = record;
            return;
        }
        self.data.accounts.push(record);
    }

    fn update_attested(&mut self, network_id: &str, name: &str) {
        if let Some(record) = self
            .data
            .accounts
            .iter_mut()
            .find(|record| record.network == network_id && record.name == name)
        {
            record.attested = true;
        }
    }

    fn auto_candidate(&self, network_id: &str, period: u32) -> Option<AccountRecord> {
        self.data
            .accounts
            .iter()
            .find(|record| {
                record.network == network_id
                    && record.attested
                    && !record.exhausted_statement_periods.contains(&period)
            })
            .cloned()
    }

    fn next_auto_name(&self, network_id: &str) -> String {
        let mut index = 1usize;
        loop {
            let name = format!("auto-{index}");
            if !self
                .data
                .accounts
                .iter()
                .any(|record| record.network == network_id && record.name == name)
            {
                return name;
            }
            index += 1;
        }
    }

    pub fn mark_exhausted(&mut self, network_id: &str, name: &str, period: u32) -> Result<()> {
        let Some(record) = self
            .data
            .accounts
            .iter_mut()
            .find(|record| record.network == network_id && record.name == name)
        else {
            return Ok(());
        };
        record.exhausted_statement_periods.insert(period);
        self.save()
    }
}

pub async fn resolve_signer(config: ResolveSignerConfig<'_>) -> Result<ResolvedSigner> {
    if let Some(mnemonic) = config.mnemonic {
        let entropy = mnemonic_entropy(&mnemonic)?;
        return Ok(ResolvedSigner {
            entropy,
            account_name: None,
            lite_username: None,
            auto_managed: false,
        });
    }

    let mut store = AccountStore::load(config.base_path)?;
    if let Some(name) = config.account {
        let record = store
            .get(config.network.id, &name)
            .cloned()
            .with_context(|| format!("account {name:?} not found for {}", config.network.id))?;
        ensure_record_ready(&mut store, config.network, &record).await?;
        return resolved_from_record(record, false);
    }

    let period = current_statement_period()?;
    if let Some(record) = store.auto_candidate(config.network.id, period) {
        ensure_record_ready(&mut store, config.network, &record).await?;
        return resolved_from_record(record, true);
    }

    let record = create_auto_account(
        &mut store,
        config.network,
        config
            .lite_username_prefix
            .as_deref()
            .unwrap_or(DEFAULT_USERNAME_PREFIX),
    )
    .await?;
    resolved_from_record(record, true)
}

pub fn mark_account_exhausted(
    base_path: &Path,
    network_id: &str,
    name: &str,
    period: u32,
) -> Result<()> {
    AccountStore::load(base_path)?.mark_exhausted(network_id, name, period)
}

pub fn current_statement_period() -> Result<u32> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before UNIX epoch")?
        .as_secs();
    Ok(alloc::slot::current_period(now))
}

async fn create_auto_account(
    store: &mut AccountStore,
    network: NetworkConfig,
    username_prefix: &str,
) -> Result<AccountRecord> {
    validate_username_prefix(username_prefix)?;
    let name = store.next_auto_name(network.id);
    let mnemonic = Mnemonic::generate(12)
        .context("generate BIP-39 mnemonic")?
        .to_string();
    let identity = identity_from_mnemonic(&mnemonic)?;

    for attempt in 0..8 {
        let lite_username = generated_username(username_prefix, attempt);
        if !attestation::lite_username_available(network.identity_backend_base, &lite_username)
            .await
            .with_context(|| format!("check lite username {lite_username:?} availability"))?
        {
            continue;
        }

        let mut record = AccountRecord {
            name: name.clone(),
            network: network.id.to_string(),
            mnemonic: mnemonic.clone(),
            lite_username,
            public_key_hex: format!("0x{}", hex::encode(identity.public_key)),
            address: identity.address.clone(),
            created_at_unix: now_unix(),
            attested: false,
            exhausted_statement_periods: BTreeSet::new(),
        };
        store.upsert(record.clone());
        store.save()?;

        info!(
            account = %record.name,
            network = %record.network,
            lite_username = %record.lite_username,
            address = %record.address,
            "created auto signer account"
        );

        attest_record(network, &record).await?;
        wait_for_ring_membership(network.people_ws, &identity.entropy).await?;
        record.attested = true;
        store.upsert(record.clone());
        store.save()?;
        return Ok(record);
    }

    bail!("could not find an available lite username for prefix {username_prefix:?}");
}

async fn ensure_record_ready(
    store: &mut AccountStore,
    network: NetworkConfig,
    record: &AccountRecord,
) -> Result<()> {
    let identity = identity_from_mnemonic(&record.mnemonic)?;
    if !record.attested {
        attest_record(network, record).await?;
        store.update_attested(network.id, &record.name);
        store.save()?;
    }
    wait_for_ring_membership(network.people_ws, &identity.entropy).await
}

async fn attest_record(network: NetworkConfig, record: &AccountRecord) -> Result<()> {
    let entropy = mnemonic_entropy(&record.mnemonic)?;
    let registered = attestation::attest(&attestation::AttestConfig {
        backend_base: network.identity_backend_base.to_string(),
        people_ws: network.people_ws.to_string(),
        entropy,
        username_base: record.lite_username.clone(),
    })
    .await
    .with_context(|| format!("attest account {}", record.name))?;
    info!(
        account = %record.name,
        lite_username = %record.lite_username,
        registered,
        "signer account attested"
    );
    Ok(())
}

async fn wait_for_ring_membership(people_ws: &str, entropy: &[u8]) -> Result<()> {
    const MAX_ATTEMPTS: usize = 90;
    const SLEEP: Duration = Duration::from_secs(4);

    let bandersnatch = alloc::bandersnatch_entropy(entropy);
    for attempt in 1..=MAX_ATTEMPTS {
        let rpc = alloc::rpc::RpcClient::connect(people_ws).await?;
        let metadata = alloc::fetch_metadata(&rpc)
            .await
            .map_err(anyhow::Error::msg)?;
        let current = alloc::ring::read_current_ring_index(&rpc)
            .await
            .map_err(anyhow::Error::msg)?;
        if alloc::find_including_ring(&rpc, &metadata, bandersnatch, current)
            .await
            .map_err(anyhow::Error::msg)?
            .is_some()
        {
            return Ok(());
        }
        if attempt < MAX_ATTEMPTS {
            warn!(
                attempt,
                max_attempts = MAX_ATTEMPTS,
                "signer account not in a LitePeople ring yet"
            );
            tokio::time::sleep(SLEEP).await;
        }
    }
    bail!("signer account did not appear in a LitePeople ring");
}

fn resolved_from_record(record: AccountRecord, auto_managed: bool) -> Result<ResolvedSigner> {
    let entropy = mnemonic_entropy(&record.mnemonic)?;
    Ok(ResolvedSigner {
        entropy,
        account_name: Some(record.name),
        lite_username: Some(record.lite_username),
        auto_managed,
    })
}

struct SignerIdentity {
    entropy: Vec<u8>,
    public_key: [u8; 32],
    address: String,
}

fn identity_from_mnemonic(mnemonic: &str) -> Result<SignerIdentity> {
    let entropy = mnemonic_entropy(mnemonic)?;
    let candidate = derive_sr25519_hard_path(&entropy, &["wallet", "sso"])
        .map_err(|err| anyhow::anyhow!("//wallet//sso derivation failed: {err}"))?;
    let public_key = candidate.public.to_bytes();
    Ok(SignerIdentity {
        entropy,
        public_key,
        address: product_public_key_to_address(public_key),
    })
}

fn mnemonic_entropy(mnemonic: &str) -> Result<Vec<u8>> {
    Ok(Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy())
}

fn validate_username_prefix(prefix: &str) -> Result<()> {
    if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_lowercase()) {
        bail!("--lite-username-prefix must contain lowercase ASCII letters only");
    }
    Ok(())
}

fn generated_username(prefix: &str, attempt: usize) -> String {
    let mut username = prefix.to_string();
    let mut seed = now_unix()
        ^ u64::from(std::process::id())
        ^ ((attempt as u64) << 32)
        ^ (prefix.len() as u64);
    while username.len() < prefix.len().max(6) + 6 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        username.push((b'a' + (seed % 26) as u8) as char);
    }
    username
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)
    }
}
