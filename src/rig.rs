//! Rig files: the unit of configuration.
//!
//! A rig names a kernel profile and optional scopes outside the workspace. It
//! is data, never a settings screen (ADR 0001).

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Access a rig's extra root asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum Access {
    #[default]
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "ro",
            Self::ReadWrite => "rw",
        }
    }
}

/// A resolved scope outside the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    pub path: PathBuf,
    pub access: Access,
}

/// A rig as the host runs it: every path absolute and validated.
#[derive(Debug, Clone)]
pub struct Rig {
    pub name: String,
    pub title: String,
    pub description: String,
    pub profile: PathBuf,
    pub extra_roots: Vec<Root>,
    /// The file this rig came from.
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRig {
    name: String,
    title: String,
    #[serde(default)]
    description: String,
    profile: PathBuf,
    #[serde(default)]
    extra_root: Vec<RawRoot>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoot {
    path: PathBuf,
    #[serde(default)]
    access: Access,
}

impl Rig {
    pub fn load(path: &Path) -> Result<Self, RigError> {
        let text = fs::read_to_string(path).map_err(|error| RigError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Self::parse(&text, path)
    }

    /// Parses a rig and resolves its paths against the file it came from.
    pub fn parse(text: &str, path: &Path) -> Result<Self, RigError> {
        let raw: RawRig = toml::from_str(text).map_err(|error| RigError::Parse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        validate_name(&raw.name, path)?;

        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let profile = if raw.profile.is_absolute() {
            normalize(&raw.profile)
        } else {
            normalize(&base.join(&raw.profile))
        };

        let mut extra_roots = Vec::with_capacity(raw.extra_root.len());
        for root in raw.extra_root {
            let expanded = expand_home(&root.path);
            if !expanded.is_absolute() {
                return Err(RigError::Invalid {
                    path: path.to_path_buf(),
                    message: format!(
                        "extra_root `{}` must be absolute or start with `~`",
                        root.path.display()
                    ),
                });
            }
            extra_roots.push(Root {
                path: normalize(&expanded),
                access: root.access,
            });
        }

        Ok(Self {
            name: raw.name,
            title: raw.title,
            description: raw.description,
            profile,
            extra_roots,
            path: path.to_path_buf(),
        })
    }
}

fn validate_name(name: &str, path: &Path) -> Result<(), RigError> {
    let mut chars = name.chars();
    let starts = matches!(chars.next(), Some(c) if c.is_ascii_lowercase());
    let rest = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if starts && rest && !name.is_empty() {
        return Ok(());
    }
    Err(RigError::Invalid {
        path: path.to_path_buf(),
        message: format!(
            "rig name `{name}` must be lowercase letters, digits and `-`, starting with a letter"
        ),
    })
}

/// Every rig found across the search path.
pub struct Discovery {
    /// Rigs that will run, sorted by name.
    pub rigs: Vec<Rig>,
    /// A rig whose name was already claimed by an earlier directory.
    pub shadowed: Vec<Rig>,
    /// Directories that were searched but do not exist.
    pub missing: Vec<PathBuf>,
    /// Files that could not be loaded. One bad rig does not hide the rest.
    pub errors: Vec<RigError>,
}

/// Directories searched for rigs, in order.
pub fn search_dirs() -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os("KEELSON_RIGS") {
        return std::env::split_paths(&value).collect();
    }
    let mut dirs = vec![PathBuf::from("rigs")];
    if let Some(state) = state_dir() {
        dirs.push(state.join("rigs"));
    }
    dirs
}

/// The host state directory: `$KEELSON_HOME`, else `~/.keelson`.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("KEELSON_HOME") {
        return Some(PathBuf::from(value));
    }
    home_dir().map(|home| home.join(".keelson"))
}

pub fn discover(dirs: &[PathBuf]) -> Discovery {
    let mut discovery = Discovery {
        rigs: Vec::new(),
        shadowed: Vec::new(),
        missing: Vec::new(),
        errors: Vec::new(),
    };
    let mut seen: HashSet<String> = HashSet::new();

    for dir in dirs {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => {
                discovery.missing.push(dir.clone());
                continue;
            }
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .map(|ext| ext.eq_ignore_ascii_case("toml"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();

        for file in files {
            match Rig::load(&file) {
                Ok(rig) => {
                    if seen.insert(rig.name.clone()) {
                        discovery.rigs.push(rig);
                    } else {
                        discovery.shadowed.push(rig);
                    }
                }
                Err(error) => discovery.errors.push(error),
            }
        }
    }

    discovery.rigs.sort_by(|a, b| a.name.cmp(&b.name));
    discovery
}

pub fn home_dir() -> Option<PathBuf> {
    let keys: &[&str] = if cfg!(windows) {
        &["USERPROFILE", "HOME"]
    } else {
        &["HOME", "USERPROFILE"]
    };
    keys.iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| !path.as_os_str().is_empty())
}

/// Collapses `.` and `..` without touching the filesystem, so a rig prints the
/// path a person would write. `..` never climbs above a root or a `..`.
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let climbed = match out.components().next_back() {
                    Some(Component::Normal(_)) => out.pop(),
                    _ => false,
                };
                if !climbed {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn expand_home(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        return home_dir().unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

#[derive(Debug)]
pub enum RigError {
    Io { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    Invalid { path: PathBuf, message: String },
}

impl fmt::Display for RigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(formatter, "{}: {message}", path.display()),
            Self::Parse { path, message } => write!(formatter, "{}: {message}", path.display()),
            Self::Invalid { path, message } => write!(formatter, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for RigError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Rig, RigError> {
        Rig::parse(text, Path::new("rigs/test.toml"))
    }

    #[test]
    fn a_minimal_rig_parses_with_defaults() {
        let rig = parse("name = \"coding\"\ntitle = \"Coding\"\nprofile = \"p.toml\"\n")
            .expect("the rig parses");

        assert_eq!(rig.name, "coding");
        assert!(rig.extra_roots.is_empty());
        assert_eq!(rig.profile, PathBuf::from("rigs/p.toml"));
    }

    #[test]
    fn a_relative_profile_path_is_normalized() {
        let rig = parse("name = \"a\"\ntitle = \"A\"\nprofile = \"../profiles/a.toml\"\n")
            .expect("the rig parses");

        assert_eq!(rig.profile, PathBuf::from("profiles/a.toml"));
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        let error = parse("name = \"a\"\ntitle = \"A\"\nprofile = \"p.toml\"\nextra = 1\n")
            .expect_err("an unknown key is refused");
        assert!(error.to_string().contains("unknown field"), "{error}");
    }

    #[test]
    fn a_relative_extra_root_is_an_error() {
        let error = parse(
            "name = \"a\"\ntitle = \"A\"\nprofile = \"p.toml\"\n[[extra_root]]\npath = \"notes\"\n",
        )
        .expect_err("a relative extra root is refused");
        assert!(error.to_string().contains("must be absolute"), "{error}");
    }

    #[test]
    fn a_name_must_be_a_handle() {
        let error = parse("name = \"Coding App\"\ntitle = \"A\"\nprofile = \"p.toml\"\n")
            .expect_err("a name with a space is refused");
        assert!(error.to_string().contains("must be lowercase"), "{error}");
    }

    #[test]
    fn discovery_keeps_the_first_rig_of_a_name() {
        let first = tempfile::TempDir::new().expect("a temp dir");
        let second = tempfile::TempDir::new().expect("a temp dir");
        let body = "name = \"coding\"\ntitle = \"Coding\"\nprofile = \"p.toml\"\n";
        fs::write(first.path().join("coding.toml"), body).expect("a rig");
        fs::write(second.path().join("coding.toml"), body).expect("a rig");

        let discovery = discover(&[first.path().to_path_buf(), second.path().to_path_buf()]);

        assert_eq!(discovery.rigs.len(), 1);
        assert_eq!(discovery.shadowed.len(), 1);
        assert!(discovery.rigs[0].path.starts_with(first.path()));
    }

    #[test]
    fn a_broken_rig_does_not_hide_the_others() {
        let dir = tempfile::TempDir::new().expect("a temp dir");
        fs::write(
            dir.path().join("good.toml"),
            "name = \"good\"\ntitle = \"Good\"\nprofile = \"p.toml\"\n",
        )
        .expect("a rig");
        fs::write(dir.path().join("bad.toml"), "name = \"bad\"\n").expect("a rig");

        let discovery = discover(&[dir.path().to_path_buf()]);

        assert_eq!(discovery.rigs.len(), 1);
        assert_eq!(discovery.errors.len(), 1);
    }
}
