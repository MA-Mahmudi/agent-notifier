use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub destinations: Vec<Destination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Destination {
    Ntfy {
        name: String,
        enabled: bool,
        server: String,
        topic: String,
        #[serde(default)]
        token_account: Option<String>,
    },
    Webhook {
        name: String,
        enabled: bool,
        url: String,
        #[serde(default)]
        token_account: Option<String>,
    },
}
impl Destination {
    pub fn name(&self) -> &str {
        match self {
            Self::Ntfy { name, .. } | Self::Webhook { name, .. } => name,
        }
    }
    pub fn enabled(&self) -> bool {
        match self {
            Self::Ntfy { enabled, .. } | Self::Webhook { enabled, .. } => *enabled,
        }
    }
}

pub fn load(path: &Path) -> anyhow::Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    toml::from_str(&std::fs::read_to_string(path)?).context("parse config.toml")
}
pub fn save(path: &Path, c: &Config) -> anyhow::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?
    }
    atomic_write(path, toml::to_string_pretty(c)?.as_bytes())
}
pub fn atomic_write(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, data)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agent-notifier/config.toml")
}
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agent-notifier")
}
pub fn secret_set(account: &str, token: &str) -> anyhow::Result<()> {
    keyring::Entry::new("io.github.mmmohebi.AgentNotifier", account)?
        .set_password(token)
        .context("Secret Service is unavailable")
}
pub fn secret_get(account: &str) -> anyhow::Result<String> {
    keyring::Entry::new("io.github.mmmohebi.AgentNotifier", account)?
        .get_password()
        .context("Secret Service is unavailable")
}
pub fn secret_delete(account: &str) {
    if let Ok(e) = keyring::Entry::new("io.github.mmmohebi.AgentNotifier", account) {
        let _ = e.delete_credential();
    }
}
