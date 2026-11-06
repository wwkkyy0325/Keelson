//! ADR 0002: the engine must not know the terminal.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn the_engine_has_no_terminal_dependency() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/engine");
    let mut files = Vec::new();
    collect(&root, &mut files);
    assert!(!files.is_empty(), "the engine directory is empty");

    for file in files {
        let text = fs::read_to_string(&file).expect("an engine source file");
        for banned in ["ratatui", "crossterm"] {
            assert!(
                !text.contains(banned),
                "{} mentions `{banned}`",
                file.display()
            );
        }
    }
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the engine directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
