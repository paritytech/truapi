use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use bip39::Mnemonic;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use truapi_server::host_logic::product_account::{
    derive_sr25519_hard_path, product_public_key_to_address,
};

use crate::attestation;
use crate::network::NetworkConfig;
use truapi_server::statement_allowance as alloc;

const ACCOUNT_STORE_FILE: &str = "accounts.json";
const ACCOUNT_STORE_LOCK_FILE: &str = "accounts.json.lock";
const DEFAULT_USERNAME_PREFIX: &str = "headless";

/// Signer material selected for a signing-host session.
#[derive(Debug, Clone)]
pub struct ResolvedSigner {
    /// BIP-39 entropy backing the selected signer account.
    pub entropy: Vec<u8>,
    /// Stored account name when this came from `accounts.json`.
    pub account_name: Option<String>,
    /// Lite username attested for the signer, when managed by the CLI.
    pub lite_username: Option<String>,
    /// Whether the account was selected from the CLI-managed auto pool.
    pub auto_managed: bool,
}

/// Inputs for resolving a signing-host account.
#[derive(Debug, Clone)]
pub struct ResolveSignerConfig<'a> {
    /// Directory containing the local account store.
    pub base_path: &'a Path,
    /// Network whose identity backend and People chain should be used.
    pub network: NetworkConfig,
    /// Explicit mnemonic. When present, the account store is not used.
    pub mnemonic: Option<String>,
    /// Named stored account. Mutually exclusive with `mnemonic`.
    pub account: Option<String>,
    /// Prefix for generated Lite usernames in auto mode.
    pub lite_username_prefix: Option<String>,
}

/// Stored signer account record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    /// Stable local account name, for example `auto-1`.
    pub name: String,
    /// Network id this account belongs to.
    pub network: String,
    /// BIP-39 mnemonic for this local test signer.
    pub mnemonic: String,
    /// Lite username registered through the identity backend.
    pub lite_username: String,
    /// Hex-encoded `//wallet//sso` public key.
    pub public_key_hex: String,
    /// SS58 address for the `//wallet//sso` public key.
    pub address: String,
    /// Creation timestamp.
    pub created_at_unix: u64,
    /// Whether registration and ring readiness completed.
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

/// Local JSON account store for CLI-managed signer accounts.
pub struct AccountStore {
    path: PathBuf,
    data: AccountStoreData,
}

struct AccountStoreLock {
    file: fs::File,
}

impl AccountStoreLock {
    fn acquire(base_path: &Path) -> Result<Self> {
        fs::create_dir_all(base_path).with_context(|| format!("create {}", base_path.display()))?;
        let path = base_path.join(ACCOUNT_STORE_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open lock {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock {}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for AccountStoreLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
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

    fn pending_auto_candidate(&self, network_id: &str) -> Option<AccountRecord> {
        self.data
            .accounts
            .iter()
            .find(|record| record.network == network_id && !record.attested)
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

    let _lock = AccountStoreLock::acquire(config.base_path)?;
    let mut store = AccountStore::load(config.base_path)?;
    if let Some(name) = config.account {
        let record = store
            .get(config.network.id, &name)
            .cloned()
            .with_context(|| format!("account {name:?} not found for {}", config.network.id))?;
        let record = ensure_record_ready(&mut store, config.network, &record).await?;
        return resolved_from_record(record, false);
    }

    let period = current_statement_period()?;
    if let Some(record) = store.auto_candidate(config.network.id, period) {
        let record = ensure_record_ready(&mut store, config.network, &record).await?;
        return resolved_from_record(record, true);
    }

    if let Some(record) = store.pending_auto_candidate(config.network.id) {
        let refreshed = ensure_record_ready(&mut store, config.network, &record).await?;
        return resolved_from_record(refreshed, true);
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

/// Resolve an already-provisioned signer from local state without network
/// attestation or ring-membership checks.
pub fn resolve_cached_signer(
    base_path: &Path,
    network_id: &str,
    account: Option<&str>,
) -> Result<Option<ResolvedSigner>> {
    let _lock = AccountStoreLock::acquire(base_path)?;
    let store = AccountStore::load(base_path)?;
    let (record, auto_managed) = if let Some(name) = account {
        (store.get(network_id, name).cloned(), false)
    } else {
        let period = current_statement_period()?;
        (store.auto_candidate(network_id, period), true)
    };
    let Some(record) =
        record.filter(|record| record.attested && resolved_lite_username(&record.lite_username))
    else {
        return Ok(None);
    };
    resolved_from_record(record, auto_managed).map(Some)
}

pub fn mark_account_exhausted(
    base_path: &Path,
    network_id: &str,
    name: &str,
    period: u32,
) -> Result<()> {
    let _lock = AccountStoreLock::acquire(base_path)?;
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

        debug!(
            account = %record.name,
            network = %record.network,
            lite_username = %record.lite_username,
            address = %record.address,
            "created auto signer account"
        );

        record.lite_username = attest_record(network, &record).await?;
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
) -> Result<AccountRecord> {
    let identity = identity_from_mnemonic(&record.mnemonic)?;
    let mut record = record.clone();
    if !record.attested {
        record.lite_username = attest_record(network, &record).await?;
        record.attested = true;
    } else {
        record.lite_username =
            attestation::registered_lite_username(network.people_ws, &identity.entropy)
                .await
                .with_context(|| format!("resolve Lite username for account {}", record.name))?;
    }
    if store
        .get(network.id, &record.name)
        .is_none_or(|stored| stored.lite_username != record.lite_username || !stored.attested)
    {
        store.upsert(record.clone());
        store.save()?;
    }
    wait_for_ring_membership(network.people_ws, &identity.entropy).await?;
    Ok(record)
}

async fn attest_record(network: NetworkConfig, record: &AccountRecord) -> Result<String> {
    let entropy = mnemonic_entropy(&record.mnemonic)?;
    let lite_username = attestation::attest(&attestation::AttestConfig {
        backend_base: network.identity_backend_base.to_string(),
        people_ws: network.people_ws.to_string(),
        entropy,
        username_base: record.lite_username.clone(),
    })
    .await
    .with_context(|| format!("attest account {}", record.name))?;
    debug!(
        account = %record.name,
        requested_lite_username = %record.lite_username,
        assigned_lite_username = %lite_username,
        "signer account attested"
    );
    Ok(lite_username)
}

fn resolved_lite_username(username: &str) -> bool {
    username
        .rsplit_once('.')
        .is_some_and(|(name, discriminator)| !name.is_empty() && !discriminator.is_empty())
}

async fn wait_for_ring_membership(people_ws: &str, entropy: &[u8]) -> Result<()> {
    const MAX_ATTEMPTS: usize = 10;
    const SLEEP: Duration = Duration::from_secs(4);

    let bandersnatch = alloc::bandersnatch_entropy(entropy);
    let mut metadata = None;
    for attempt in 1..=MAX_ATTEMPTS {
        crate::terminal_ui::update_activity(
            "signer",
            "Setting up signer",
            Some(format!(
                "Waiting for LitePeople ring membership · attempt {attempt}/{MAX_ATTEMPTS}"
            )),
            crate::terminal_ui::ActivityState::Running,
        );
        let rpc = match alloc::rpc::RpcClient::connect(people_ws).await {
            Ok(rpc) => rpc,
            Err(err) => {
                warn!(
                    attempt,
                    max_attempts = MAX_ATTEMPTS,
                    error = %err,
                    "could not connect while checking LitePeople ring membership"
                );
                sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
                continue;
            }
        };
        if metadata.is_none() {
            match alloc::fetch_metadata(&rpc).await {
                Ok(fetched) => metadata = Some(fetched),
                Err(err) => {
                    warn!(
                        attempt,
                        max_attempts = MAX_ATTEMPTS,
                        error = %err,
                        "could not fetch metadata while checking LitePeople ring membership"
                    );
                    sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
                    continue;
                }
            }
        }
        let metadata_ref = metadata.as_ref().expect("metadata is initialized");
        let current = match alloc::ring::read_current_ring_index(&rpc).await {
            Ok(current) => current,
            Err(err) => {
                warn!(
                    attempt,
                    max_attempts = MAX_ATTEMPTS,
                    error = %err,
                    "could not read current LitePeople ring"
                );
                sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
                continue;
            }
        };
        match alloc::find_including_ring(&rpc, metadata_ref, bandersnatch, current).await {
            Ok(Some(_)) => {
                crate::terminal_ui::update_activity(
                    "signer",
                    "Setting up signer",
                    Some("LitePeople ring membership ready".to_string()),
                    crate::terminal_ui::ActivityState::Running,
                );
                return Ok(());
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    attempt,
                    max_attempts = MAX_ATTEMPTS,
                    error = %err,
                    "could not scan LitePeople rings"
                );
            }
        }
        sleep_ring_poll(attempt, MAX_ATTEMPTS, SLEEP).await;
    }
    bail!("signer account did not appear in a LitePeople ring");
}

async fn sleep_ring_poll(attempt: usize, max_attempts: usize, sleep: Duration) {
    if attempt < max_attempts {
        debug!(
            attempt,
            max_attempts, "signer account not in a LitePeople ring yet"
        );
        tokio::time::sleep(sleep).await;
    }
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
    let tmp_path = temp_path(path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        sync_parent(path)
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp_path, bytes)?;
        let _ = fs::remove_file(path);
        fs::rename(&tmp_path, path)
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(ACCOUNT_STORE_FILE);
    path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()))
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn record(name: &str, network: &str, attested: bool) -> AccountRecord {
        AccountRecord {
            name: name.to_string(),
            network: network.to_string(),
            mnemonic: MNEMONIC.to_string(),
            lite_username: format!("{name}lite.01"),
            public_key_hex: "0x00".to_string(),
            address: "5GrwvaEF5zXb26Fz9rcQpDWSKfwVwqNxyvE9uZunJMtBEw2s".to_string(),
            created_at_unix: 1,
            attested,
            exhausted_statement_periods: BTreeSet::new(),
        }
    }

    #[test]
    fn auto_candidate_skips_pending_and_exhausted_accounts() {
        let mut store = AccountStore {
            path: PathBuf::from("accounts.json"),
            data: AccountStoreData::default(),
        };
        let mut exhausted = record("auto-1", "paseo-next-v2", true);
        exhausted.exhausted_statement_periods.insert(7);
        store.upsert(exhausted);
        store.upsert(record("auto-2", "paseo-next-v2", false));
        store.upsert(record("auto-3", "paseo-next-v2", true));

        assert_eq!(
            store
                .auto_candidate("paseo-next-v2", 7)
                .map(|record| record.name),
            Some("auto-3".to_string())
        );
    }

    #[test]
    fn pending_auto_candidate_reuses_failed_onboarding_record() {
        let mut store = AccountStore {
            path: PathBuf::from("accounts.json"),
            data: AccountStoreData::default(),
        };
        store.upsert(record("auto-1", "paseo-next-v2", false));
        store.upsert(record("auto-2", "other", false));

        assert_eq!(
            store
                .pending_auto_candidate("paseo-next-v2")
                .map(|record| record.name),
            Some("auto-1".to_string())
        );
    }

    #[test]
    fn save_roundtrips_account_store() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        store.upsert(record("auto-1", "paseo-next-v2", true));
        store.save()?;

        let loaded = AccountStore::load(dir.path())?;

        assert_eq!(
            loaded
                .get("paseo-next-v2", "auto-1")
                .map(|record| record.name.as_str()),
            Some("auto-1")
        );
        assert!(!temp_path(&dir.path().join(ACCOUNT_STORE_FILE)).exists());
        Ok(())
    }

    #[test]
    fn cached_signer_resolves_without_network_access() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        store.upsert(record("auto-1", "paseo-next-v2", true));
        store.save()?;

        let signer =
            resolve_cached_signer(dir.path(), "paseo-next-v2", None)?.expect("cached signer");

        assert_eq!(signer.account_name.as_deref(), Some("auto-1"));
        assert_eq!(signer.lite_username.as_deref(), Some("auto-1lite.01"));
        assert!(signer.auto_managed);
        Ok(())
    }

    #[test]
    fn cached_signer_ignores_legacy_username_base() -> Result<()> {
        let dir = tempdir()?;
        let mut store = AccountStore::load(dir.path())?;
        let mut stale = record("auto-1", "paseo-next-v2", true);
        stale.lite_username = "headlessabcdef".to_string();
        store.upsert(stale);
        store.save()?;

        assert!(resolve_cached_signer(dir.path(), "paseo-next-v2", None)?.is_none());
        Ok(())
    }
}
