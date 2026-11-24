//! Stamps the binary with the code it was built from (ADR 0084).
//!
//! A record that cannot name its build is a record about an unknown program.
//! The accounts under `<state>/perf/` and the step log outlive the process that
//! wrote them, and every crate here says `0.1.0` however much changed, so a
//! report of "it froze" arrives from a build nobody can identify. The stamp is
//! the commit the app was built from, the commit of the kernel it linked, and
//! `-dirty` on either tree when the sources were not committed.
//!
//! The stamp is also the list of things it is computed from: `tauri-build`
//! declares its own inputs, which switches cargo's default watch of the package
//! off, so an unwatched stamp would describe whichever build ran last.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The paths whose changes make the host a different program.
const HOST_SOURCES: &[&str] = &[
    "desktop/src",
    "desktop/dist",
    "desktop/package.json",
    "desktop/src-tauri/src",
    "desktop/src-tauri/tauri.conf.json",
];

/// The kernel is a path dependency, so its state is not in the host's commit.
const KEEL_SOURCES: &[&str] = &["crates"];

fn main() {
    let stamp = stamp();
    println!("cargo:rustc-env=KEELSON_BUILD={stamp}");
    for path in watched() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    tauri_build::build()
}

/// `keelson 0.1.0 a1b2c3d4 keel 5e6f7a8b`, each commit suffixed with `-dirty`
/// when that tree had uncommitted changes; `unknown` outside a checkout.
fn stamp() -> String {
    let version = env::var("CARGO_PKG_VERSION").expect("package version");
    let mut stamp = format!(
        "keelson {version} {}",
        commit(repo_root().as_deref(), HOST_SOURCES)
    );
    if let Some(keel) = keel_root().filter(|path| path.is_dir()) {
        stamp.push_str(&format!(" keel {}", commit(Some(&keel), KEEL_SOURCES)));
    }
    stamp
}

/// The host checkout: `desktop/src-tauri` → the repository root.
fn repo_root() -> Option<PathBuf> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").ok()?);
    manifest.parent()?.parent().map(Path::to_path_buf)
}

/// The kernel checkout the host links by path, beside this one.
fn keel_root() -> Option<PathBuf> {
    Some(repo_root()?.parent()?.join("Keel"))
}

/// `<short commit>`, or `<short commit>-dirty` when `sources` are not committed.
fn commit(root: Option<&Path>, sources: &[&str]) -> String {
    let Some(root) = root else {
        return "unknown".to_owned();
    };
    let Some(head) = head_of(root) else {
        return "unknown".to_owned();
    };
    let mut status = vec!["status", "--porcelain"];
    status.extend_from_slice(sources);
    match git(root, &status) {
        Some(out) if !out.trim().is_empty() => format!("{head}-dirty"),
        _ => head,
    }
}

/// The short hash of the checkout's HEAD.
fn head_of(root: &Path) -> Option<String> {
    let head = git(root, &["rev-parse", "--short=8", "HEAD"])?;
    let head = head.trim();
    (!head.is_empty()).then(|| head.to_owned())
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Everything the stamp is read from.
fn watched() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (root, sources) in [(repo_root(), HOST_SOURCES), (keel_root(), KEEL_SOURCES)] {
        let Some(root) = root else { continue };
        for source in sources {
            paths.push(root.join(source));
        }
        let git_dir = root.join(".git");
        paths.push(git_dir.join("HEAD"));
        paths.push(git_dir.join("index"));
        if let Some(reference) = head_ref(&git_dir) {
            paths.push(reference);
        }
    }
    paths
}

/// The file a symbolic HEAD points at, so committing moves the stamp.
fn head_ref(git_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let reference = text.trim().strip_prefix("ref:")?.trim().to_owned();
    Some(git_dir.join(reference))
}
