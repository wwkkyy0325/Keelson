//! Checks that the kernel handshake and every rig resolve.

use crate::rig::{self, Discovery};

/// Prints the state and returns the process exit code.
pub fn run(discovery: &Discovery) -> u8 {
    println!(
        "keelson {}  kernel {}  events {}  snapshot {}",
        env!("CARGO_PKG_VERSION"),
        keel_core::KERNEL_VERSION,
        keel_core::EVENT_FORMAT_VERSION,
        keel_core::SNAPSHOT_VERSION
    );

    let dirs = rig::search_dirs();
    let labels: Vec<String> = dirs.iter().map(|dir| dir.display().to_string()).collect();
    println!("rig dirs    {}", labels.join(", "));
    for missing in &discovery.missing {
        println!("            {} (not found)", missing.display());
    }

    let mut healthy = true;
    println!("rigs        {}", discovery.rigs.len());
    for rig in &discovery.rigs {
        match keel_policy::load_profile(&rig.profile) {
            Ok(config) => println!(
                "  {:<10}  {:<11}  profile {} ({:?})",
                rig.name,
                rig.ui.as_str(),
                config.profile_name,
                config.permission_level
            ),
            Err(error) => {
                healthy = false;
                println!(
                    "  {:<10}  {:<11}  profile failed: {} ({error})",
                    rig.name,
                    rig.ui.as_str(),
                    rig.profile.display()
                );
            }
        }
    }

    for rig in &discovery.shadowed {
        println!(
            "  shadowed   {} ({}) — an earlier directory wins",
            rig.name,
            rig.path.display()
        );
    }
    for error in &discovery.errors {
        healthy = false;
        println!("  problem    {error}");
    }

    if healthy {
        0
    } else {
        1
    }
}
