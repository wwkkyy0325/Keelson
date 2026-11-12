//! The explainer starter kit, embedded at build time.
//!
//! An empty project cannot render until `kit/`, `scenes/` and the data scripts
//! exist. The desktop seeds them when an explainer conversation opens; the
//! copy never overwrites a file the project already has.

use std::fs;
use std::path::Path;

use keel_core::KernelError;

include!(concat!(env!("OUT_DIR"), "/kit_assets.rs"));

/// Copies the starter kit into `workspace`, writing only files that do not
/// exist yet. Returns how many files were written.
pub fn seed_explainer_kit(workspace: &Path) -> Result<usize, KernelError> {
    let mut written = 0usize;
    for (relative, bytes) in EXPLAINER_KIT {
        let target = workspace.join(relative);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| KernelError::Io {
                message: format!("cannot create `{}`: {error}", parent.display()),
            })?;
        }
        fs::write(&target, bytes).map_err(|error| KernelError::Io {
            message: format!("cannot write `{}`: {error}", target.display()),
        })?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_the_kit_without_overwriting() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let workspace = dir.path();
        fs::create_dir_all(workspace.join("kit")).expect("the kit dir");
        fs::write(workspace.join("kit/scene.html"), "custom").expect("a custom page");
        fs::write(workspace.join("script.md"), "mine").expect("the script");

        let written = seed_explainer_kit(workspace).expect("the kit seeds");
        assert!(written > 0);
        assert_eq!(
            fs::read_to_string(workspace.join("kit/scene.html")).expect("the page"),
            "custom"
        );
        assert_eq!(
            fs::read_to_string(workspace.join("script.md")).expect("the script"),
            "mine"
        );
        for file in [
            "kit/timeline.py",
            "kit/qa.py",
            "kit/scene3d.html",
            "scenes/scene-template.js",
            "data/build.py",
        ] {
            assert!(workspace.join(file).is_file(), "{file}");
        }

        assert_eq!(
            seed_explainer_kit(workspace).expect("the kit is idempotent"),
            0
        );
    }
}
