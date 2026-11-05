//! Keelson: a shell host for the Keel kernel.
//!
//! One binary, several rigs. A rig is a file that names a kernel profile, a
//! view and optional scopes; the host loads it and gives it a face.

mod doctor;
mod rig;

use std::process::ExitCode;

const USAGE: &str = "\
keelson - a host for the Keel kernel

usage:
  keelson rig list          list the rigs on the search path
  keelson rig show <name>   print one rig as the host resolves it
  keelson doctor            check the kernel handshake and every rig
  keelson --version

A rig is a file. `KEELSON_RIGS` replaces the search path; otherwise `./rigs`
is searched, then `<home>/.keelson/rigs`.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("keelson: {message}");
            ExitCode::from(2)
        }
    }
}

fn dispatch(args: &[String]) -> Result<u8, String> {
    let command = args.first().map(String::as_str);
    let subcommand = args.get(1).map(String::as_str);
    match (command, subcommand) {
        (None, _) => {
            println!("{USAGE}");
            list()
        }
        (Some("--help" | "-h"), _) => {
            println!("{USAGE}");
            Ok(0)
        }
        (Some("--version" | "-V"), _) => {
            print_version();
            Ok(0)
        }
        (Some("doctor"), _) => Ok(doctor::run(&discover())),
        (Some("rig" | "rigs"), None | Some("list")) => list(),
        (Some("rig"), Some("show")) => {
            let name = args
                .get(2)
                .ok_or_else(|| "`rig show` needs a rig name".to_owned())?;
            show(name)
        }
        (Some(other), _) => Err(format!("unknown command `{other}`; try `keelson --help`")),
    }
}

fn discover() -> rig::Discovery {
    rig::discover(&rig::search_dirs())
}

fn list() -> Result<u8, String> {
    let discovery = discover();
    if discovery.rigs.is_empty() {
        println!("no rigs on the search path");
    }
    for rig in &discovery.rigs {
        println!("{:<12}  {:<11}  {}", rig.name, rig.ui.as_str(), rig.title);
    }
    for rig in &discovery.shadowed {
        eprintln!("shadowed {} ({})", rig.name, rig.path.display());
    }
    for error in &discovery.errors {
        eprintln!("problem {error}");
    }
    Ok(u8::from(!discovery.errors.is_empty()))
}

fn show(name: &str) -> Result<u8, String> {
    let discovery = discover();
    let rig = discovery
        .rigs
        .iter()
        .find(|rig| rig.name == name)
        .ok_or_else(|| format!("no rig named `{name}`"))?;

    println!("name        {}", rig.name);
    println!("title       {}", rig.title);
    if !rig.description.is_empty() {
        println!("description {}", rig.description);
    }
    println!("view        {}", rig.ui.as_str());
    println!("profile     {}", rig.profile.display());
    println!("source      {}", rig.path.display());
    if rig.extra_roots.is_empty() {
        println!("extra roots none");
    }
    for root in &rig.extra_roots {
        println!(
            "extra root  {} ({})",
            root.path.display(),
            root.access.as_str()
        );
    }
    Ok(0)
}

fn print_version() {
    println!(
        "keelson {} (kernel {}, events {}, snapshot {})",
        env!("CARGO_PKG_VERSION"),
        keel_core::KERNEL_VERSION,
        keel_core::EVENT_FORMAT_VERSION,
        keel_core::SNAPSHOT_VERSION
    );
}
