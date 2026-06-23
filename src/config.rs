//! Persistent configuration and credentials (`cli-spec.md` §2).
//!
//! Two files under `~/.config/wms/`:
//!   - `config.toml`      — non-secret settings, per profile;
//!   - `credentials.toml` — session token / API key, written mode `0600`.
//!
//! Resolution precedence (highest first) is applied in `context.rs`:
//!   flag > `WMS_*` env > profile config > built-in default.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, Result};

pub const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProfileSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tenant: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileSettings>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProfileCredentials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CredentialsFile {
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileCredentials>,
}

/// Loaded view of both config files plus their on-disk locations.
pub struct ConfigStore {
    dir: PathBuf,
    pub config: ConfigFile,
    pub credentials: CredentialsFile,
}

impl ConfigStore {
    fn dir() -> Result<PathBuf> {
        let base = dirs::config_dir().ok_or_else(|| {
            CliError::Usage("cannot locate a config directory for this OS".into())
        })?;
        Ok(base.join("wms"))
    }

    pub fn load() -> Result<Self> {
        let dir = Self::dir()?;
        let config = read_toml::<ConfigFile>(&dir.join("config.toml"))?.unwrap_or_default();
        let credentials =
            read_toml::<CredentialsFile>(&dir.join("credentials.toml"))?.unwrap_or_default();
        Ok(ConfigStore {
            dir,
            config,
            credentials,
        })
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    pub fn credentials_path(&self) -> PathBuf {
        self.dir.join("credentials.toml")
    }

    /// The active profile name: explicit override, else file default, else "default".
    pub fn active_profile(&self, override_name: Option<&str>) -> String {
        override_name
            .map(str::to_string)
            .or_else(|| self.config.default_profile.clone())
            .unwrap_or_else(|| DEFAULT_PROFILE.to_string())
    }

    pub fn settings(&self, profile: &str) -> ProfileSettings {
        self.config
            .profiles
            .get(profile)
            .cloned()
            .unwrap_or_default()
    }

    pub fn creds(&self, profile: &str) -> ProfileCredentials {
        self.credentials
            .profiles
            .get(profile)
            .cloned()
            .unwrap_or_default()
    }

    pub fn settings_mut(&mut self, profile: &str) -> &mut ProfileSettings {
        self.config.profiles.entry(profile.to_string()).or_default()
    }

    pub fn creds_mut(&mut self, profile: &str) -> &mut ProfileCredentials {
        self.credentials
            .profiles
            .entry(profile.to_string())
            .or_default()
    }

    pub fn set_default_profile(&mut self, profile: &str) {
        self.config.default_profile = Some(profile.to_string());
    }

    pub fn save_config(&self) -> Result<()> {
        write_toml(&self.config_path(), &self.config, false)
    }

    /// Persists credentials with restrictive (`0600`) permissions on Unix.
    pub fn save_credentials(&self) -> Result<()> {
        write_toml(&self.credentials_path(), &self.credentials, true)
    }
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let parsed = toml::from_str::<T>(&text)
                .map_err(|e| CliError::Usage(format!("invalid {}: {e}", path.display())))?;
            Ok(Some(parsed))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CliError::Other(
            anyhow::Error::new(e).context(format!("reading {}", path.display())),
        )),
    }
}

fn write_toml<T: Serialize>(path: &Path, value: &T, secret: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(value).map_err(anyhow::Error::new)?;
    std::fs::write(path, text)?;
    if secret {
        set_secret_permissions(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_secret_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_secret_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
