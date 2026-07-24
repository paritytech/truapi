//! Network-scoped signing-host session directories and current selection.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SESSION_NAME: &str = "default";
const CURRENT_SESSION_FILE: &str = "current-session";
const SESSION_INFO_FILE: &str = "session.json";

#[derive(Debug, Serialize, Deserialize)]
struct SessionInfo {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_script: Option<String>,
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self {
            version: 1,
            user_id: None,
            last_script: None,
        }
    }
}

/// Filesystem locations owned by one managed signing-host session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProfile {
    pub name: String,
    /// Core/session state directory shown by `/session`.
    pub path: PathBuf,
    /// Product-local KV directory for this named session.
    pub product_storage_dir: PathBuf,
    /// Directory containing this session's `accounts.json`.
    pub account_base_path: PathBuf,
}

/// Persistent session catalog for one network.
#[derive(Debug, Clone)]
pub struct SessionCatalog {
    base_path: PathBuf,
    network_path: PathBuf,
    role_path: PathBuf,
}

impl SessionCatalog {
    pub fn new(base_path: PathBuf, network_id: &str) -> Result<Self> {
        let base_path = absolute_path(base_path)?;
        let network_path = base_path.join(network_id);
        let role_path = network_path.join("signing-host");
        fs::create_dir_all(&role_path)
            .with_context(|| format!("create session root {}", role_path.display()))?;
        Ok(Self {
            base_path,
            network_path,
            role_path,
        })
    }

    pub fn profile(&self, name: &str) -> Result<SessionProfile> {
        validate_name(name).map_err(anyhow::Error::msg)?;
        if name == DEFAULT_SESSION_NAME {
            return Ok(SessionProfile {
                name: name.to_string(),
                path: self.role_path.clone(),
                product_storage_dir: self.role_path.join("storage").join(name),
                // Preserve the pre-session account store for compatibility.
                account_base_path: self.base_path.clone(),
            });
        }
        let identity_path = self.identity_path(name);
        let legacy_path = self.role_path.join("sessions").join(name);
        let path = if legacy_path.is_dir() && !identity_path.is_dir() {
            legacy_path
        } else {
            identity_path
        };
        let product_storage_dir = if path.starts_with(self.role_path.join("sessions")) {
            self.role_path.join("storage").join(name)
        } else {
            path.join("storage")
        };
        Ok(SessionProfile {
            name: name.to_string(),
            path: path.clone(),
            product_storage_dir,
            account_base_path: path,
        })
    }

    pub fn ensure_profile(&self, name: &str) -> Result<SessionProfile> {
        let profile = self.profile(name)?;
        fs::create_dir_all(&profile.path)
            .with_context(|| format!("create session {}", profile.path.display()))?;
        Ok(profile)
    }

    pub fn exists(&self, name: &str) -> bool {
        self.profile(name)
            .is_ok_and(|profile| name == DEFAULT_SESSION_NAME || profile.path.is_dir())
    }

    pub fn current_name(&self) -> String {
        let path = self.role_path.join(CURRENT_SESSION_FILE);
        let Ok(name) = fs::read_to_string(path) else {
            return DEFAULT_SESSION_NAME.to_string();
        };
        let name = name.trim();
        if self.exists(name) {
            name.to_string()
        } else {
            DEFAULT_SESSION_NAME.to_string()
        }
    }

    pub fn set_current(&self, name: &str) -> Result<()> {
        let profile = self.ensure_profile(name)?;
        let path = self.role_path.join(CURRENT_SESSION_FILE);
        let temporary = self.role_path.join(format!(
            ".{CURRENT_SESSION_FILE}.{}.tmp",
            std::process::id()
        ));
        fs::write(&temporary, format!("{}\n", profile.name))
            .with_context(|| format!("write current session {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("persist current session {}", path.display()))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        let sessions_path = self.role_path.join("sessions");
        match fs::read_dir(&sessions_path) {
            Ok(entries) => {
                for entry in entries.filter_map(std::result::Result::ok) {
                    if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        continue;
                    }
                    let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                        continue;
                    };
                    if validate_name(&name).is_ok() && name != DEFAULT_SESSION_NAME {
                        names.push(name);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("list sessions {}", sessions_path.display()));
            }
        }
        for entry in fs::read_dir(&self.network_path)
            .with_context(|| format!("list host profiles {}", self.network_path.display()))?
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Some(filename) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            let Some(name) = filename.strip_suffix("_signing_host") else {
                continue;
            };
            if validate_name(name).is_ok() && name != DEFAULT_SESSION_NAME {
                names.push(name.to_string());
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Move a provisional or legacy session into the user-owned host root.
    ///
    /// The public session name is the Lite username. The suffix is only a
    /// filesystem discriminator so pairing and signing state cannot collide.
    pub fn promote_to_user(
        &self,
        profile: &SessionProfile,
        user_id: &str,
    ) -> Result<SessionProfile> {
        validate_name(user_id).map_err(anyhow::Error::msg)?;
        let target_path = self.identity_path(user_id);
        if profile.path != target_path && !target_path.exists() {
            if profile.path == self.role_path {
                fs::create_dir_all(&target_path)
                    .with_context(|| format!("create user host {}", target_path.display()))?;
                migrate_default_profile(profile, &target_path)?;
            } else {
                fs::rename(&profile.path, &target_path).with_context(|| {
                    format!(
                        "move host profile {} to {}",
                        profile.path.display(),
                        target_path.display()
                    )
                })?;
                if profile.product_storage_dir.exists()
                    && !profile.product_storage_dir.starts_with(&profile.path)
                {
                    let target_storage = target_path.join("storage");
                    fs::create_dir_all(&target_path)?;
                    fs::rename(&profile.product_storage_dir, &target_storage).with_context(
                        || {
                            format!(
                                "move product storage {} to {}",
                                profile.product_storage_dir.display(),
                                target_storage.display()
                            )
                        },
                    )?;
                }
            }
        }
        let promoted = SessionProfile {
            name: user_id.to_string(),
            path: target_path.clone(),
            product_storage_dir: target_path.join("storage"),
            account_base_path: target_path,
        };
        fs::create_dir_all(&promoted.path)
            .with_context(|| format!("create user host {}", promoted.path.display()))?;
        self.store_user_id(&promoted, user_id)?;
        Ok(promoted)
    }

    pub fn cached_user_id(&self, profile: &SessionProfile) -> Result<Option<String>> {
        Ok(read_session_info(&profile.path)?
            .user_id
            .filter(|user_id| !user_id.is_empty()))
    }

    pub fn store_user_id(&self, profile: &SessionProfile, user_id: &str) -> Result<()> {
        if user_id.is_empty() {
            return Ok(());
        }
        let mut info = read_session_info(&profile.path)?;
        info.user_id = Some(user_id.to_string());
        write_session_info(&profile.path, &info)
    }

    /// Return the last script used in this session, if it still exists.
    pub fn last_script(&self, profile: &SessionProfile) -> Result<Option<PathBuf>> {
        session_last_script(&profile.path)
    }

    /// Remember the last script used in this session.
    pub fn store_last_script(&self, profile: &SessionProfile, script: &Path) -> Result<()> {
        store_session_last_script(&profile.path, script)
    }

    fn identity_path(&self, user_id: &str) -> PathBuf {
        self.network_path.join(format!("{user_id}_signing_host"))
    }
}

fn migrate_default_profile(profile: &SessionProfile, target_path: &Path) -> Result<()> {
    for name in [
        "core-storage.json",
        "product-storage.json",
        SESSION_INFO_FILE,
    ] {
        let source = profile.path.join(name);
        if source.is_file() {
            fs::rename(&source, target_path.join(name))
                .with_context(|| format!("move {}", source.display()))?;
        }
    }
    let scripts = profile.path.join("scripts");
    if scripts.is_dir() {
        fs::rename(&scripts, target_path.join("scripts"))
            .with_context(|| format!("move {}", scripts.display()))?;
    }
    if profile.product_storage_dir.is_dir() {
        fs::rename(&profile.product_storage_dir, target_path.join("storage"))
            .with_context(|| format!("move {}", profile.product_storage_dir.display()))?;
    }
    let account_store = profile.account_base_path.join("accounts.json");
    if account_store.is_file() {
        fs::copy(&account_store, target_path.join("accounts.json"))
            .with_context(|| format!("copy {}", account_store.display()))?;
    }
    Ok(())
}

/// Return the last script recorded in a host/session state directory.
pub fn session_last_script(session_path: &Path) -> Result<Option<PathBuf>> {
    let info = read_session_info(session_path)?;
    let Some(filename) = info.last_script else {
        return Ok(None);
    };
    let configured = Path::new(&filename);
    if configured.is_absolute() {
        return Ok(configured.is_file().then(|| configured.to_path_buf()));
    }

    let relative = configured;
    let mut components = relative.components();
    let Some(Component::Normal(filename)) = components.next() else {
        anyhow::bail!("session last script is not a portable filename");
    };
    if components.next().is_some() {
        anyhow::bail!("session last script must stay inside its scripts directory");
    }
    let script = session_path.join("scripts").join(filename);
    Ok(script.is_file().then_some(script))
}

/// Record a script in a host/session state directory.
///
/// Scratch scripts are stored by filename so existing session directories stay
/// portable. Explicit scripts outside that directory are stored as absolute
/// paths so a later bare `/script` reopens the same file.
pub fn store_session_last_script(session_path: &Path, script: &Path) -> Result<()> {
    let script = absolute_path(script.to_path_buf())?;
    let scripts = session_path.join("scripts");
    let stored_path = if script.parent() == Some(scripts.as_path()) {
        script
            .file_name()
            .and_then(|filename| filename.to_str())
            .context("scratch script filename is not valid UTF-8")?
    } else {
        script.to_str().context("script path is not valid UTF-8")?
    };
    let mut info = read_session_info(session_path)?;
    info.last_script = Some(stored_path.to_string());
    write_session_info(session_path, &info)
}

fn read_session_info(session_path: &Path) -> Result<SessionInfo> {
    let path = session_path.join(SESSION_INFO_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SessionInfo::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("read session metadata {}", path.display()));
        }
    };
    serde_json::from_str(&text)
        .with_context(|| format!("decode session metadata {}", path.display()))
}

fn write_session_info(session_path: &Path, info: &SessionInfo) -> Result<()> {
    fs::create_dir_all(session_path)
        .with_context(|| format!("create session {}", session_path.display()))?;
    let path = session_path.join(SESSION_INFO_FILE);
    let temporary = session_path.join(format!(".{SESSION_INFO_FILE}.{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(info)?;
    fs::write(&temporary, format!("{text}\n"))
        .with_context(|| format!("write session metadata {}", temporary.display()))?;
    fs::rename(&temporary, &path)
        .with_context(|| format!("persist session metadata {}", path.display()))?;
    Ok(())
}

/// Validate a portable session name before using it as a path component.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err("session name must contain between 1 and 64 characters".to_string());
    }
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return Err("session name cannot be empty".to_string());
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("session name must start with a lowercase letter or digit".to_string());
    }
    if !bytes.all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(
            "session name may contain only lowercase letters, digits, `.`, `_`, and `-`"
                .to_string(),
        );
    }
    if matches!(name, "." | "..") {
        return Err("session name cannot be `.` or `..`".to_string());
    }
    Ok(())
}

/// Validate a user-selectable session name.
///
/// `default` remains a private bootstrap profile and must not be exposed as a
/// session users can create or switch to.
pub fn validate_selectable_name(name: &str) -> Result<(), String> {
    validate_name(name)?;
    if name == DEFAULT_SESSION_NAME {
        return Err(
            "session name `default` is reserved for bootstrap state; choose a user session name such as `alice`"
                .to_string(),
        );
    }
    Ok(())
}

/// Select the Lite username prefix for auto-accounts owned by a session.
///
/// Lite username bases accept lowercase ASCII letters only, while session
/// names additionally accept digits and separators. Preserve the recognizable
/// alphabetic portion of a named session and use a neutral fallback when its
/// name contains no letters. The default session retains the account manager's
/// historical default unless an explicit prefix was supplied.
pub fn lite_username_prefix(name: &str, explicit: Option<&str>) -> Option<String> {
    if let Some(explicit) = explicit {
        return Some(explicit.to_string());
    }
    if name == DEFAULT_SESSION_NAME {
        return None;
    }
    let prefix: String = name
        .bytes()
        .filter(u8::is_ascii_lowercase)
        .map(char::from)
        .collect();
    Some(if prefix.is_empty() {
        "session".to_string()
    } else {
        prefix
    })
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(std::env::current_dir()
        .context("resolve current directory")?
        .join(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_names_that_could_escape_the_session_root() {
        for invalid in ["", ".", "..", "Alice", "two words", "../escape", "a/b"] {
            assert!(validate_name(invalid).is_err(), "accepted {invalid:?}");
        }
        assert!(validate_name("alice-2.test").is_ok());
    }

    #[test]
    fn default_is_internal_and_not_user_selectable() {
        assert!(validate_name(DEFAULT_SESSION_NAME).is_ok());
        assert!(validate_selectable_name(DEFAULT_SESSION_NAME).is_err());
        assert!(validate_selectable_name("alice").is_ok());
    }

    #[test]
    fn derives_lite_username_prefix_from_session_name() {
        assert_eq!(
            lite_username_prefix("pgtest", None).as_deref(),
            Some("pgtest")
        );
        assert_eq!(
            lite_username_prefix("pg-test_2", None).as_deref(),
            Some("pgtest")
        );
        assert_eq!(
            lite_username_prefix("123", None).as_deref(),
            Some("session")
        );
        assert_eq!(lite_username_prefix(DEFAULT_SESSION_NAME, None), None);
        assert_eq!(
            lite_username_prefix("pgtest", Some("custom")).as_deref(),
            Some("custom")
        );
    }

    #[test]
    fn persists_and_lists_the_current_network_session() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        catalog.set_current("alice")?;

        assert_eq!(catalog.current_name(), "alice");
        assert_eq!(catalog.list()?, vec!["alice"]);
        let profile = catalog.profile("alice")?;
        assert!(profile.path.ends_with("testnet/alice_signing_host"));
        assert!(
            profile
                .product_storage_dir
                .ends_with("testnet/alice_signing_host/storage")
        );
        assert_eq!(profile.path, profile.account_base_path);
        Ok(())
    }

    #[test]
    fn promotes_a_provisional_profile_to_the_username_host_root() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let provisional = catalog.ensure_profile("pgtest")?;
        fs::write(provisional.path.join("accounts.json"), "{}")?;
        fs::create_dir_all(&provisional.product_storage_dir)?;
        fs::write(provisional.product_storage_dir.join("product.json"), "{}")?;

        let promoted = catalog.promote_to_user(&provisional, "alice.dot")?;

        assert_eq!(promoted.name, "alice.dot");
        assert!(promoted.path.ends_with("testnet/alice.dot_signing_host"));
        assert!(promoted.path.join("accounts.json").is_file());
        assert!(promoted.product_storage_dir.join("product.json").is_file());
        assert!(!provisional.path.exists());
        Ok(())
    }

    #[test]
    fn default_profile_preserves_legacy_storage_locations() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.profile(DEFAULT_SESSION_NAME)?;

        assert_eq!(profile.account_base_path, temporary.path());
        assert!(profile.path.ends_with("testnet/signing-host"));
        assert!(
            profile
                .product_storage_dir
                .ends_with("testnet/signing-host/storage/default")
        );
        Ok(())
    }

    #[test]
    fn session_metadata_preserves_user_id_and_last_script() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.ensure_profile("alice")?;
        let scripts = profile.path.join("scripts");
        fs::create_dir_all(&scripts)?;
        let script = scripts.join("script.ts");
        fs::write(&script, "console.log('test');")?;

        assert_eq!(catalog.cached_user_id(&profile)?, None);
        assert_eq!(catalog.last_script(&profile)?, None);
        catalog.store_last_script(&profile, &script)?;
        catalog.store_user_id(&profile, "alice.dot")?;
        let replacement = scripts.join("replacement.ts");
        fs::write(&replacement, "console.log('replacement');")?;
        catalog.store_last_script(&profile, &replacement)?;

        assert_eq!(
            catalog.cached_user_id(&profile)?.as_deref(),
            Some("alice.dot")
        );
        assert_eq!(
            catalog.last_script(&profile)?.as_deref(),
            Some(replacement.as_path())
        );
        let metadata = fs::read_to_string(profile.path.join(SESSION_INFO_FILE))?;
        assert!(metadata.contains("\"user_id\": \"alice.dot\""));
        assert!(metadata.contains("\"last_script\": \"replacement.ts\""));
        assert!(profile.path.join(SESSION_INFO_FILE).is_file());
        Ok(())
    }

    #[test]
    fn stale_or_escaping_last_script_is_never_opened() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.ensure_profile("alice")?;
        let scripts = profile.path.join("scripts");
        fs::create_dir_all(&scripts)?;
        let stale = scripts.join("missing.ts");
        catalog.store_last_script(&profile, &stale)?;

        assert_eq!(catalog.last_script(&profile)?, None);
        fs::write(
            profile.path.join(SESSION_INFO_FILE),
            r#"{"version":1,"last_script":"../outside.ts"}"#,
        )?;
        assert!(catalog.last_script(&profile).is_err());
        Ok(())
    }

    #[test]
    fn session_metadata_preserves_an_explicit_script_outside_scratch_storage() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.ensure_profile("alice")?;
        let script = temporary.path().join("product-script.ts");
        fs::write(&script, "console.log('product');")?;

        catalog.store_last_script(&profile, &script)?;

        assert_eq!(
            catalog.last_script(&profile)?.as_deref(),
            Some(script.as_path())
        );
        let metadata = fs::read_to_string(profile.path.join(SESSION_INFO_FILE))?;
        assert!(metadata.contains(script.to_str().context("temporary path is not UTF-8")?));
        Ok(())
    }
}
