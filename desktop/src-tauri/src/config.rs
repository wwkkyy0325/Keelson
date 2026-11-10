//! Desktop preferences and the OS keychain. The API key never enters the
//! config file; only the non-secret connection settings do.

use std::fs;
use std::path::{Path, PathBuf};

use keelson::rig;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "keelson";
const ACCOUNT: &str = "api-key";
const RECENT_PROJECTS: usize = 8;

/// A recently opened workspace. It is a shortcut, not a permission grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    pub path: String,
    pub name: String,
    pub opened_at: u64,
}

/// The last manual connection, so switching back to a local or self-hosted
/// server does not mean retyping its address and model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualEndpoint {
    pub provider: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopConfig {
    pub onboarded: bool,
    pub provider: String,
    /// The catalog provider id, used to identify the key's owner.
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub model: String,
    /// The selected model's hard context capacity, when the catalog knows it.
    pub model_context: Option<u32>,
    pub locale: String,
    pub last_workspace: String,
    /// Newest first, capped at eight.
    pub projects: Vec<ProjectEntry>,
    /// Which provider the key was last saved for.
    pub key_provider: String,
    /// Providers that have a key in the keychain. The keychain is the truth;
    /// this list only lets the settings panel skip asking for a saved key.
    pub key_providers: Vec<String>,
    /// The last manual endpoint, if one was ever saved.
    pub manual: Option<ManualEndpoint>,
    /// The rig file to run. Empty means "whichever rig the provider implies",
    /// which is what an install from before the picker gets.
    pub rig: String,
    /// A host permission override (`workspace_auto` or `full_auto`). `None`
    /// keeps the rig profile's level.
    pub permission: Option<String>,
    /// A SearXNG base URL for web search when the model provider has no
    /// search endpoint of its own. Not a secret.
    pub search_url: Option<String>,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            onboarded: false,
            provider: "openai-compatible".to_owned(),
            provider_id: String::new(),
            provider_name: String::new(),
            base_url: "https://api.deepseek.com".to_owned(),
            model: "deepseek-flash".to_owned(),
            model_context: None,
            locale: "zh".to_owned(),
            last_workspace: String::new(),
            projects: Vec::new(),
            key_provider: String::new(),
            key_providers: Vec::new(),
            manual: None,
            rig: String::new(),
            permission: None,
            search_url: None,
        }
    }
}

/// Moves a workspace to the front of the recent list.
pub fn remember_project(config: &mut DesktopConfig, path: &str, now: u64) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    config.projects.retain(|project| project.path != path);
    config.projects.insert(
        0,
        ProjectEntry {
            path: path.to_owned(),
            name: project_name(path),
            opened_at: now,
        },
    );
    config.projects.truncate(RECENT_PROJECTS);
}

/// Projects for the switcher. An install from before the list existed is
/// seeded from its last workspace without a write.
pub fn recent_projects(config: &DesktopConfig) -> Vec<ProjectEntry> {
    if !config.projects.is_empty() {
        return config.projects.clone();
    }
    let path = config.last_workspace.trim();
    if path.is_empty() {
        return Vec::new();
    }
    vec![ProjectEntry {
        path: path.to_owned(),
        name: project_name(path),
        opened_at: 0,
    }]
}

fn project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_owned()
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
    let mut config: DesktopConfig = fs::read_to_string(config_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    seed_key_providers(&mut config);
    config
}

/// An install from before the per-provider slots kept one key; its owner is
/// the provider recorded when it was saved.
fn seed_key_providers(config: &mut DesktopConfig) {
    if config.key_providers.is_empty() && !config.key_provider.trim().is_empty() {
        config.key_providers.push(config.key_provider.clone());
    }
}

pub fn save_config(config: &DesktopConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    fs::write(&path, text).map_err(|error| error.to_string())
}

/// Keys live in one keychain slot per provider, so switching between the
/// local server and a cloud provider keeps both instead of overwriting one.
fn account_for(provider: &str) -> String {
    let provider = provider.trim();
    if provider.is_empty() {
        ACCOUNT.to_owned()
    } else {
        format!("{ACCOUNT}:{provider}")
    }
}

pub fn save_key(provider: &str, key: &str) -> Result<(), String> {
    write_key(&account_for(provider), key)
}

/// The key for one provider.
///
/// A key saved before the slots existed lives in the single account and is
/// moved into its slot on first read; only the provider recorded when it was
/// saved can claim it.
pub fn load_key(provider: &str, owner: &str) -> Result<Option<String>, String> {
    if let Some(key) = read_key(&account_for(provider))? {
        return Ok(Some(key));
    }
    if provider.trim().is_empty() || owner != provider {
        return Ok(None);
    }
    let Some(key) = read_key(ACCOUNT)? else {
        return Ok(None);
    };
    let _ = write_key(&account_for(provider), &key);
    Ok(Some(key))
}

fn write_key(account: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account).map_err(|error| error.to_string())?;
    entry.set_password(key).map_err(|error| error.to_string())
}

fn read_key(account: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE, account).map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Ok(Some(key)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_moves_to_the_front_without_duplicates() {
        let mut config = DesktopConfig::default();
        remember_project(&mut config, "C:/work/one", 1);
        remember_project(&mut config, "C:/work/two", 2);
        remember_project(&mut config, "C:/work/one", 3);

        assert_eq!(config.projects.len(), 2);
        assert_eq!(config.projects[0].path, "C:/work/one");
        assert_eq!(config.projects[0].name, "one");
        assert_eq!(config.projects[0].opened_at, 3);
    }

    #[test]
    fn the_recent_list_is_capped() {
        let mut config = DesktopConfig::default();
        for index in 0..12 {
            remember_project(&mut config, &format!("C:/work/{index}"), index);
        }

        assert_eq!(config.projects.len(), RECENT_PROJECTS);
        assert_eq!(config.projects[0].path, "C:/work/11");
    }

    #[test]
    fn an_old_install_seeds_its_key_providers() {
        let mut config = DesktopConfig {
            key_provider: "deepseek".to_owned(),
            ..DesktopConfig::default()
        };

        seed_key_providers(&mut config);

        assert_eq!(config.key_providers, ["deepseek"]);
    }

    #[test]
    fn recorded_key_providers_are_kept() {
        let mut config = DesktopConfig {
            key_provider: "deepseek".to_owned(),
            key_providers: vec!["custom".to_owned(), "deepseek".to_owned()],
            ..DesktopConfig::default()
        };

        seed_key_providers(&mut config);

        assert_eq!(config.key_providers, ["custom", "deepseek"]);
    }

    #[test]
    fn an_old_install_seeds_its_recent_list() {
        let config = DesktopConfig {
            last_workspace: "C:/Users/me/Documents/Keelson".to_owned(),
            ..DesktopConfig::default()
        };

        let projects = recent_projects(&config);

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Keelson");
    }
}
