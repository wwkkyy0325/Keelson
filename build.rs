//! Embeds the explainer starter kit so the desktop can seed a project without
//! shipping a separate resource directory.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let kit = manifest.join("kits/explainer");
    let mut files = Vec::new();
    collect(&kit, &kit, &mut files);
    files.sort();

    let out = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("kit_assets.rs");
    let mut code = String::from(
        "/// Every file in `kits/explainer`, relative to the kit root.\n\
         pub static EXPLAINER_KIT: &[(&str, &[u8])] = &[\n",
    );
    for relative in &files {
        let portable = relative.replace('\\', "/");
        code.push_str(&format!(
            "    ({portable:?}, include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/kits/explainer/{portable}\"))),\n"
        ));
        println!("cargo:rerun-if-changed={}", kit.join(relative).display());
    }
    code.push_str("];\n");
    fs::write(&out, code).expect("write kit assets");
    println!("cargo:rerun-if-changed={}", kit.display());
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.is_file() {
            if let Ok(relative) = path.strip_prefix(root) {
                files.push(relative.to_string_lossy().into_owned());
            }
        }
    }
}
