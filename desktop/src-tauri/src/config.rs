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
    /// Which provider the stored key belongs to. Empty means "unknown".
    pub key_provider: String,
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
