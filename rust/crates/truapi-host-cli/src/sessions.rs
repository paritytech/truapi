//! Network-scoped signing-host session directories and current selection.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SESSION_NAME: &str = "default";
const CURRENT_SESSION_FILE: &str = "current-session";
const SESSION_INFO_FILE: &str = "session.json";

#[derive(Debug, Serialize, Deserialize)]
struct SessionInfo {
    version: u32,
    user_id: String,
}

/// Filesystem locations owned by one managed signing-host session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProfile {
    pub name: String,
    /// Product/core storage directory shown by `/session`.
    pub path: PathBuf,
    /// Directory containing this session's `accounts.json`.
    pub account_base_path: PathBuf,
}

/// Persistent session catalog for one network.
#[derive(Debug, Clone)]
pub struct SessionCatalog {
    base_path: PathBuf,
    role_path: PathBuf,
}

impl SessionCatalog {
    pub fn new(base_path: PathBuf, network_id: &str) -> Result<Self> {
        let base_path = absolute_path(base_path)?;
        let role_path = base_path.join(network_id).join("signing-host");
        fs::create_dir_all(&role_path)
            .with_context(|| format!("create session root {}", role_path.display()))?;
        Ok(Self {
            base_path,
            role_path,
        })
    }

    pub fn profile(&self, name: &str) -> Result<SessionProfile> {
        validate_name(name).map_err(anyhow::Error::msg)?;
        if name == DEFAULT_SESSION_NAME {
            return Ok(SessionProfile {
                name: name.to_string(),
                path: self.role_path.clone(),
                // Preserve the pre-session account store for compatibility.
                account_base_path: self.base_path.clone(),
            });
        }
        let path = self.role_path.join("sessions").join(name);
        Ok(SessionProfile {
            name: name.to_string(),
            path: path.clone(),
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
        let mut names = vec![DEFAULT_SESSION_NAME.to_string()];
        let sessions_path = self.role_path.join("sessions");
        let entries = match fs::read_dir(&sessions_path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(names),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("list sessions {}", sessions_path.display()));
            }
        };
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
        names.sort();
        Ok(names)
    }

    pub fn cached_user_id(&self, profile: &SessionProfile) -> Result<Option<String>> {
        let path = profile.path.join(SESSION_INFO_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read session identity {}", path.display()));
            }
        };
        let info: SessionInfo = serde_json::from_str(&text)
            .with_context(|| format!("decode session identity {}", path.display()))?;
        Ok((!info.user_id.is_empty()).then_some(info.user_id))
    }

    pub fn store_user_id(&self, profile: &SessionProfile, user_id: &str) -> Result<()> {
        if user_id.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&profile.path)
            .with_context(|| format!("create session {}", profile.path.display()))?;
        let path = profile.path.join(SESSION_INFO_FILE);
        let temporary = profile
            .path
            .join(format!(".{SESSION_INFO_FILE}.{}.tmp", std::process::id()));
        let text = serde_json::to_string_pretty(&SessionInfo {
            version: 1,
            user_id: user_id.to_string(),
        })?;
        fs::write(&temporary, format!("{text}\n"))
            .with_context(|| format!("write session identity {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("persist session identity {}", path.display()))?;
        Ok(())
    }
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
    fn persists_and_lists_the_current_network_session() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        catalog.set_current("alice")?;

        assert_eq!(catalog.current_name(), "alice");
        assert_eq!(catalog.list()?, vec!["alice", "default"]);
        let profile = catalog.profile("alice")?;
        assert!(
            profile
                .path
                .ends_with("testnet/signing-host/sessions/alice")
        );
        assert_eq!(profile.path, profile.account_base_path);
        Ok(())
    }

    #[test]
    fn default_profile_preserves_legacy_storage_locations() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.profile(DEFAULT_SESSION_NAME)?;

        assert_eq!(profile.account_base_path, temporary.path());
        assert!(profile.path.ends_with("testnet/signing-host"));
        Ok(())
    }

    #[test]
    fn session_user_id_round_trips_in_the_profile_path() -> Result<()> {
        let temporary = tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.ensure_profile("alice")?;

        assert_eq!(catalog.cached_user_id(&profile)?, None);
        catalog.store_user_id(&profile, "alice.dot")?;

        assert_eq!(
            catalog.cached_user_id(&profile)?.as_deref(),
            Some("alice.dot")
        );
        assert!(profile.path.join(SESSION_INFO_FILE).is_file());
        Ok(())
    }
}
