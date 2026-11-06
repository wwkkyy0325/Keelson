//! Desktop preferences and the OS keychain. The API key never enters the
//! config file; only the non-secret connection settings do.

use std::fs;
use std::path::PathBuf;

use keelson::rig;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "keelson";
const ACCOUNT: &str = "api-key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopConfig {
    pub onboarded: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub locale: String,
    pub last_workspace: String,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            onboarded: false,
            provider: "anthropic-compatible".to_owned(),
            base_url: "https://api.deepseek.com/anthropic".to_owned(),
            model: "deepseek-flash[1M]".to_owned(),
            locale: "zh".to_owned(),
            last_workspace: String::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    let state = rig::state_dir().unwrap_or_else(|| {
        rig::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".keelson")
    });
    state.join("desktop.json")
}

pub fn load_config() -> DesktopConfig {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &DesktopConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    fs::write(&path, text).map_err(|error| error.to_string())
}

pub fn save_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
    entry.set_password(key).map_err(|error| error.to_string())
}

pub fn load_key() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Ok(Some(key)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
