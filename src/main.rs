//! Keelson: a shell host for the Keel kernel.
//!
//! One binary, several rigs. A rig is a file that names a kernel profile, a
//! view and optional scopes; the host loads it and gives it a face.

mod doctor;
mod engine;
mod render;
mod rig;
mod view;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
keelson - a host for the Keel kernel

usage:
  keelson run <rig>         run one turn through the rig's view
  keelson rig list          list the rigs on the search path
  keelson rig show <name>   print one rig as the host resolves it
  keelson doctor            check the kernel handshake and every rig
  keelson --version

run options:
  --input TEXT              the turn input (default: read stdin; a transcript
                            run prefills the prompt instead)
  --session-id ID           continue a session (default: default)
  --store PATH              session file (default: <state>/sessions/<id>.json)
  --lang auto|zh|en         transcript language (default: auto)

A rig is a file. `KEELSON_RIGS` replaces the search path; otherwise `./rigs`
is searched, then `<state>/rigs`. The state directory is `$KEELSON_HOME`, else
`~/.keelson`.
";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args).await {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("keelson: {message}");
            ExitCode::from(2)
        }
    }
}

async fn dispatch(args: &[String]) -> Result<u8, String> {
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
        (Some("run"), None) => Err("`run` needs a rig name".to_owned()),
        (Some("run"), Some(_)) => run_rig(args).await,
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

async fn run_rig(args: &[String]) -> Result<u8, String> {
    let name = args
        .get(1)
        .ok_or_else(|| "`run` needs a rig name".to_owned())?;
    let discovery = discover();
    for error in &discovery.errors {
        eprintln!("problem {error}");
    }
    let rig = discovery
        .rigs
        .into_iter()
        .find(|rig| &rig.name == name)
        .ok_or_else(|| format!("no rig named `{name}`"))?;

    let mut input = None;
    let mut session_id = "default".to_owned();
    let mut store_path = None;
    let mut lang = "auto".to_owned();
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                input = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for `--input`".to_owned())?
                        .clone(),
                );
                index += 2;
            }
            "--session-id" => {
                session_id = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for `--session-id`".to_owned())?
                    .clone();
                validate_session_id(&session_id)?;
                index += 2;
            }
            "--store" => {
                store_path = Some(PathBuf::from(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for `--store`".to_owned())?,
                ));
                index += 2;
            }
            "--lang" => {
                lang = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for `--lang`".to_owned())?
                    .clone();
                if !matches!(lang.as_str(), "auto" | "zh" | "en") {
                    return Err("`--lang` must be auto, zh or en".to_owned());
                }
                index += 2;
            }
            other => return Err(format!("unknown argument `{other}`; try `keelson --help`")),
        }
    }

    let workspace = std::env::current_dir().map_err(|error| error.to_string())?;
    let state_dir = rig::state_dir().unwrap_or_else(|| workspace.join(".keel"));
    let store_path = store_path.unwrap_or_else(|| {
        state_dir
            .join("sessions")
            .join(format!("{session_id}.json"))
    });

    match rig.ui {
        rig::UiMode::Oneshot => {
            let input = match input {
                Some(input) => input,
                None => read_stdin()?,
            };
            let input = input.trim_end_matches(['\r', '\n']).to_owned();
            if input.trim().is_empty() {
                return Err("the input is empty".to_owned());
            }
            let report = engine::run(engine::RunRequest {
                rig,
                workspace,
                state_dir,
                store_path,
                input,
            })
            .await
            .map_err(|error| error.to_string())?;
            Ok(view::oneshot::render(&report))
        }
        rig::UiMode::Transcript => {
            if !std::io::stdin().is_terminal() {
                return Err("the transcript view needs an interactive terminal; \
                     use a oneshot rig for pipes"
                    .to_owned());
            }
            let (port, interactions) = view::transcript::interaction_pair();
            let (deltas, ui) = tokio::sync::mpsc::channel(256);
            let engine = engine::Engine::open(engine::OpenRequest {
                rig,
                workspace,
                state_dir,
                store_path,
                interactions: std::sync::Arc::new(port),
                deltas: Some(deltas),
            })
            .map_err(|error| error.to_string())?;
            let text = view::ui_text::Text::new(view::ui_text::Lang::resolve(Some(&lang)));
            view::transcript::run(view::transcript::TranscriptHost {
                text,
                engine: std::sync::Arc::new(engine),
                prefill: input,
                interactions,
                ui,
            })
            .await
            .map_err(|error| error.to_string())
        }
        rig::UiMode::Chat => Err(format!(
            "rig `{name}` uses the `chat` view, which arrives later"
        )),
    }
}

fn read_stdin() -> Result<String, String> {
    use std::io::{IsTerminal, Read};

    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err("no input; pass `--input` or pipe it on stdin".to_owned());
    }
    let mut text = String::new();
    stdin
        .lock()
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read stdin: {error}"))?;
    Ok(text)
}

/// A session id becomes a file name, so it stays a simple handle.
fn validate_session_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
    if valid {
        return Ok(());
    }
    Err(format!(
        "session id `{id}` must be letters, digits, `-` and `_`"
    ))
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
