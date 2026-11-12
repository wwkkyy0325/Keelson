//! Series layout: one self-contained folder per episode, a machine-readable
//! index at the root and a release archive.
//!
//! The renderer works on one episode directory at a time, so `new_episode`
//! copies the starter kit into the episode. `archive_episode` copies the
//! finished cut and its sidecars to `releases/<episode>/`, prunes the frames
//! and updates `series.json`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use keel_core::{
    AccessKind, ArgValue, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool, ToolArgs,
    ToolCtx, ToolError, ToolResult, ToolSchema,
};
use serde_json::{json, Value};

use super::seed_explainer_kit;

const SERIES_INDEX: &str = "series.json";
const EPISODES: &str = "episodes";
const RELEASES: &str = "releases";

/// Creates the next episode folder, seeds the kit and records it in the index.
pub struct NewEpisodeTool;

impl NewEpisodeTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for NewEpisodeTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "new_episode".to_owned(),
            description: "Create the next episode of a series: a self-contained \
                          `episodes/NNN-slug/` folder with the starter kit, recorded in \
                          `series.json`. Pass a lowercase ASCII slug and the episode title. \
                          Write the brief, script, storyboard and scenes inside that folder; \
                          never create episode files at the series root."
                .to_owned(),
            risk: RiskLevel::Mutating,
            params: vec![
                ParamSpec {
                    name: "slug".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "title".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "number".to_owned(),
                    kind: ParamKind::Integer,
                    required: false,
                },
                ParamSpec {
                    name: "series_title".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(vec![
            PermissionRequest {
                tool: "new_episode".to_owned(),
                access: AccessKind::Write,
                path: Some(PathBuf::from(EPISODES)),
                detail: Some("a new episode folder".to_owned()),
            },
            PermissionRequest {
                tool: "new_episode".to_owned(),
                access: AccessKind::Write,
                path: Some(PathBuf::from(SERIES_INDEX)),
                detail: Some("the series index".to_owned()),
            },
        ])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        match create_episode(&ctx.workspace_root, &args) {
            Ok(message) => ToolResult::success(ctx.call_id, "new_episode", message),
            Err(error) => ToolResult::failure(ctx.call_id, "new_episode", error),
        }
    }
}

/// Copies a finished episode's deliverables into `releases/`, deletes the
/// frame sequence and marks the episode in `series.json`.
pub struct ArchiveEpisodeTool;

impl ArchiveEpisodeTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ArchiveEpisodeTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "archive_episode".to_owned(),
            description: "Archive one rendered episode: copy `out/film.mp4`, subtitles, \
                          cover and contact sheet to `releases/<episode>/`, write a \
                          `meta.json`, delete `out/frames`, and update `series.json`. \
                          `episode` is the folder name or a slug; `status` is `rendered` \
                          (default) or `published`. Run it only after the final cut passes \
                          QA."
            .to_owned(),
            risk: RiskLevel::Mutating,
            params: vec![
                ParamSpec {
                    name: "episode".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "status".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(vec![
            PermissionRequest {
                tool: "archive_episode".to_owned(),
                access: AccessKind::Write,
                path: Some(PathBuf::from(EPISODES)),
                detail: Some("delete the frame sequence".to_owned()),
            },
            PermissionRequest {
                tool: "archive_episode".to_owned(),
                access: AccessKind::Write,
                path: Some(PathBuf::from(RELEASES)),
                detail: Some("copy the release".to_owned()),
            },
            PermissionRequest {
                tool: "archive_episode".to_owned(),
                access: AccessKind::Write,
                path: Some(PathBuf::from(SERIES_INDEX)),
                detail: Some("update the series index".to_owned()),
            },
        ])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        match archive_episode(&ctx.workspace_root, &args) {
            Ok(message) => ToolResult::success(ctx.call_id, "archive_episode", message),
            Err(error) => ToolResult::failure(ctx.call_id, "archive_episode", error),
        }
    }
}

fn create_episode(root: &Path, args: &ToolArgs) -> Result<String, ToolError> {
    let slug = args.require_str("slug")?.trim().to_ascii_lowercase();
    if !valid_slug(&slug) {
        return Err(ToolError::invalid_arguments(
            "`slug` must be lowercase ASCII letters, digits or hyphens, at most 64 characters",
        ));
    }
    let title = args.require_str("title")?.trim();
    if title.is_empty() {
        return Err(ToolError::invalid_arguments("`title` must not be empty"));
    }
    let number = match args.get("number") {
        Some(ArgValue::Integer(value)) if *value > 0 => u32::try_from(*value)
            .map_err(|_| ToolError::invalid_arguments("`number` is too large"))?,
        Some(ArgValue::Integer(_)) => {
            return Err(ToolError::invalid_arguments(
                "`number` must be a positive integer",
            ))
        }
        Some(_) => {
            return Err(ToolError::invalid_arguments(
                "`number` must be a positive integer",
            ))
        }
        None => next_number(root)?,
    };
    let name = format!("{number:03}-{slug}");
    let dir = root.join(EPISODES).join(&name);
    if dir.exists() {
        return Err(ToolError::invalid_arguments(format!(
            "`{}` already exists; pick the next number or a different slug",
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|error| ToolError::io(format!("cannot create {}: {error}", dir.display())))?;
    let seeded = seed_explainer_kit(&dir).map_err(|error| ToolError::io(error.to_string()))?;
    copy_shared_assets(root, &dir)?;

    let mut series = load_series(root)?;
    let series_title = args
        .get("series_title")
        .and_then(|value| match value {
            ArgValue::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
            _ => None,
        })
        .or_else(|| {
            series
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| workspace_name(root));
    let object = series
        .as_object_mut()
        .ok_or_else(|| ToolError::io("series.json is not an object"))?;
    object.insert("title".to_owned(), Value::String(series_title));
    object.insert("updatedAt".to_owned(), Value::String(today()));
    let episodes = object
        .entry("episodes")
        .or_insert_with(|| Value::Array(Vec::new()));
    let list = episodes
        .as_array_mut()
        .ok_or_else(|| ToolError::io("series.json `episodes` is not a list"))?;
    list.push(json!({
        "number": number,
        "slug": slug,
        "title": title,
        "status": "idea",
        "dir": format!("{EPISODES}/{name}"),
        "release": Value::Null,
        "video": Value::Null,
        "createdAt": today()
    }));
    save_series(root, &series)?;
    Ok(format!(
        "created {EPISODES}/{name} and seeded {seeded} kit files; series.json updated"
    ))
}

fn archive_episode(root: &Path, args: &ToolArgs) -> Result<String, ToolError> {
    let requested = args.require_str("episode")?;
    let status = match args.get("status") {
        Some(ArgValue::String(text)) => text.trim().to_owned(),
        Some(_) => {
            return Err(ToolError::invalid_arguments(
                "`status` must be `rendered` or `published`",
            ))
        }
        None => "rendered".to_owned(),
    };
    if status != "rendered" && status != "published" {
        return Err(ToolError::invalid_arguments(
            "`status` must be `rendered` or `published`",
        ));
    }
    let dir = find_episode(root, requested)?;
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ToolError::io("the episode folder has no name"))?
        .to_owned();
    let out = dir.join("out");
    let film = out.join("film.mp4");
    if !film.is_file() {
        return Err(ToolError::not_found(
            "out/film.mp4 is missing; render and QA the cut first",
        ));
    }

    let release = root.join(RELEASES).join(&name);
    std::fs::create_dir_all(&release)
        .map_err(|error| ToolError::io(format!("cannot create {}: {error}", release.display())))?;
    copy_file(&film, &release.join("film.mp4"))?;
    for (source, target) in [
        ("out/all.srt", "subtitles.srt"),
        ("out/subtitles.srt", "subtitles.srt"),
        ("out/contact-sheet.jpg", "contact-sheet.jpg"),
        ("out/cover.png", "cover.png"),
        ("publish.md", "publish.md"),
    ] {
        let source = dir.join(source);
        if source.is_file() {
            copy_file(&source, &release.join(target))?;
        }
    }
    let meta = json!({
        "title": title_of(root, &name),
        "episode": name,
        "status": status,
        "archivedAt": today(),
        "source": format!("{EPISODES}/{name}"),
    });
    std::fs::write(
        release.join("meta.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&meta).unwrap_or_default()
        ),
    )
    .map_err(|error| ToolError::io(format!("cannot write release meta: {error}")))?;

    let (frames, bytes) = remove_tree(&out.join("frames"));
    update_series_on_archive(root, &dir, &name, &status, &release)?;
    Ok(format!(
        "archived {EPISODES}/{name} to {RELEASES}/{name}; removed {frames} frame files ({:.1} MB)",
        bytes as f64 / (1024.0 * 1024.0)
    ))
}

fn next_number(root: &Path) -> Result<u32, ToolError> {
    let mut highest = 0u32;
    if let Ok(entries) = std::fs::read_dir(root.join(EPISODES)) {
        for entry in entries.flatten() {
            if let Some(number) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.split_once('-'))
                .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
            {
                highest = highest.max(number);
            }
        }
    }
    if let Ok(series) = load_series(root) {
        if let Some(list) = series.get("episodes").and_then(Value::as_array) {
            for entry in list {
                if let Some(number) = entry.get("number").and_then(Value::as_u64) {
                    highest = highest.max(u32::try_from(number).unwrap_or(u32::MAX));
                }
            }
        }
    }
    Ok(highest.saturating_add(1).max(1))
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn load_series(root: &Path) -> Result<Value, ToolError> {
    let path = root.join(SERIES_INDEX);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| ToolError::io(format!("series.json is not valid JSON: {error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({
            "title": workspace_name(root),
            "episodes": []
        })),
        Err(error) => Err(ToolError::io(format!("cannot read series.json: {error}"))),
    }
}

fn save_series(root: &Path, series: &Value) -> Result<(), ToolError> {
    let text = serde_json::to_string_pretty(series)
        .map_err(|error| ToolError::io(format!("cannot serialize series.json: {error}")))?;
    std::fs::write(root.join(SERIES_INDEX), format!("{text}\n"))
        .map_err(|error| ToolError::io(format!("cannot write series.json: {error}")))
}

fn workspace_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("series")
        .to_owned()
}

fn find_episode(root: &Path, requested: &str) -> Result<PathBuf, ToolError> {
    let requested = requested.trim();
    if requested.is_empty() {
        return Err(ToolError::invalid_arguments("`episode` must not be empty"));
    }
    let episodes = root.join(EPISODES);
    if requested.contains(['/', '\\']) {
        let path = root.join(requested);
        let canonical = std::fs::canonicalize(&path)
            .map_err(|_| ToolError::not_found(format!("no episode folder at {requested}")))?;
        let base = std::fs::canonicalize(&episodes)
            .map_err(|error| ToolError::io(format!("cannot read the episodes folder: {error}")))?;
        if canonical.starts_with(&base) && canonical.is_dir() {
            return Ok(canonical);
        }
        return Err(ToolError::invalid_arguments(
            "`episode` must name a folder under episodes/",
        ));
    }
    let entries = std::fs::read_dir(&episodes)
        .map_err(|error| ToolError::io(format!("cannot read the episodes folder: {error}")))?;
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name == requested
                || name.ends_with(&format!("-{requested}"))
                || name
                    .split_once('-')
                    .and_then(|(number, _)| number.parse::<u32>().ok())
                    .is_some_and(|number| requested.parse::<u32>().ok() == Some(number))
        })
        .collect();
    matches.sort();
    match matches.len() {
        0 => Err(ToolError::not_found(format!(
            "no episode matches `{requested}`"
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(ToolError::invalid_arguments(format!(
            "`{requested}` matches more than one episode; use the folder name"
        ))),
    }
}

fn copy_shared_assets(root: &Path, episode: &Path) -> Result<(), ToolError> {
    let source = root.join("assets");
    if !source.is_dir() {
        return Ok(());
    }
    copy_tree_missing(&source, &episode.join("assets"))
}

fn copy_tree_missing(source: &Path, target: &Path) -> Result<(), ToolError> {
    std::fs::create_dir_all(target)
        .map_err(|error| ToolError::io(format!("cannot create {}: {error}", target.display())))?;
    let entries = std::fs::read_dir(source)
        .map_err(|error| ToolError::io(format!("cannot read {}: {error}", source.display())))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree_missing(&from, &to)?;
        } else if from.is_file() && !to.exists() {
            copy_file(&from, &to)?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path) -> Result<(), ToolError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            ToolError::io(format!("cannot create {}: {error}", parent.display()))
        })?;
    }
    std::fs::copy(source, target).map_err(|error| {
        ToolError::io(format!(
            "cannot copy {} to {}: {error}",
            source.display(),
            target.display()
        ))
    })?;
    Ok(())
}

fn remove_tree(path: &Path) -> (usize, u64) {
    let mut files = 0usize;
    let mut bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                let (nested_files, nested_bytes) = remove_tree(&child);
                files += nested_files;
                bytes += nested_bytes;
            } else if child.is_file() {
                files += 1;
                bytes = bytes.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
            }
        }
    }
    let _ = std::fs::remove_dir_all(path);
    (files, bytes)
}

fn update_series_on_archive(
    root: &Path,
    dir: &Path,
    name: &str,
    status: &str,
    release: &Path,
) -> Result<(), ToolError> {
    let mut series = load_series(root)?;
    let object = series
        .as_object_mut()
        .ok_or_else(|| ToolError::io("series.json is not an object"))?;
    object.insert("updatedAt".to_owned(), Value::String(today()));
    let list = object
        .entry("episodes")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| ToolError::io("series.json `episodes` is not a list"))?;
    let dir_text = format!("{EPISODES}/{name}");
    let release_text = format!("{RELEASES}/{name}");
    let video_text = format!("{RELEASES}/{name}/film.mp4");
    let entry = list.iter_mut().find(|entry| {
        entry.get("dir").and_then(Value::as_str) == Some(dir_text.as_str())
            || entry.get("slug").and_then(Value::as_str) == Some(name)
    });
    let title = title_of(root, name);
    match entry {
        Some(entry) => {
            let object = entry
                .as_object_mut()
                .ok_or_else(|| ToolError::io("series.json episode is not an object"))?;
            object.insert("status".to_owned(), Value::String(status.to_owned()));
            object.insert("release".to_owned(), Value::String(release_text));
            object.insert("video".to_owned(), Value::String(video_text));
            object.insert("archivedAt".to_owned(), Value::String(today()));
        }
        None => list.push(json!({
            "number": number_of(dir),
            "slug": name,
            "title": title,
            "status": status,
            "dir": dir_text,
            "release": release_text,
            "video": video_text,
            "createdAt": today(),
            "archivedAt": today()
        })),
    }
    if !release.is_dir() {
        return Err(ToolError::io("the release folder was not created"));
    }
    save_series(root, &series)
}

fn number_of(dir: &Path) -> u32 {
    dir.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
        .unwrap_or(0)
}

fn title_of(root: &Path, name: &str) -> String {
    let expected = format!("{EPISODES}/{name}");
    load_series(root)
        .ok()
        .and_then(|series| {
            series
                .get("episodes")
                .and_then(Value::as_array)
                .and_then(|list| {
                    list.iter()
                        .find(|entry| entry.get("dir").and_then(Value::as_str) == Some(&expected))
                        .and_then(|entry| entry.get("title"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
        })
        .unwrap_or_else(|| name.to_owned())
}

fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::{CancelToken, ToolCallId, TurnId};

    fn ctx(root: &Path) -> ToolCtx {
        ToolCtx {
            turn_id: TurnId::new(1),
            call_id: ToolCallId::new(1),
            workspace_root: root.to_path_buf(),
            cancel: CancelToken::new(),
        }
    }

    fn args(pairs: &[(&str, ArgValue)]) -> ToolArgs {
        let mut args = ToolArgs::new();
        for (name, value) in pairs {
            args.insert(*name, value.clone());
        }
        args
    }

    #[tokio::test]
    async fn a_new_episode_gets_the_kit_and_an_index_entry() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let tool = NewEpisodeTool::new();
        let args = args(&[
            ("slug", ArgValue::String("school-unit".to_owned())),
            ("title", ArgValue::String("学校单元".to_owned())),
            ("series_title", ArgValue::String("责任链条".to_owned())),
        ]);

        let result = tool.call(args, &ctx(dir.path())).await;
        assert!(result.is_success(), "{:?}", result.error());

        let episode = dir.path().join("episodes/001-school-unit");
        assert!(episode.join("kit/scene.html").is_file());
        assert!(episode.join("scenes/scene-template.js").is_file());
        let series: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(SERIES_INDEX)).expect("the index"),
        )
        .expect("the index parses");
        assert_eq!(series["title"], "责任链条");
        assert_eq!(series["episodes"][0]["dir"], "episodes/001-school-unit");
        assert_eq!(series["episodes"][0]["status"], "idea");
    }

    #[tokio::test]
    async fn archiving_copies_the_cut_and_removes_the_frames() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let create = NewEpisodeTool::new();
        let created = create
            .call(
                args(&[
                    ("slug", ArgValue::String("school-unit".to_owned())),
                    ("title", ArgValue::String("学校单元".to_owned())),
                ]),
                &ctx(dir.path()),
            )
            .await;
        assert!(created.is_success(), "{:?}", created.error());

        let episode = dir.path().join("episodes/001-school-unit");
        std::fs::create_dir_all(episode.join("out/frames")).expect("the frame folder");
        std::fs::write(episode.join("out/film.mp4"), b"film").expect("the cut");
        std::fs::write(episode.join("out/all.srt"), b"subs").expect("the subtitles");
        std::fs::write(episode.join("out/frames/0000.png"), b"frame").expect("a frame");
        std::fs::write(episode.join("publish.md"), b"release copy").expect("publish notes");

        let archive = ArchiveEpisodeTool::new();
        let archived = archive
            .call(
                args(&[
                    ("episode", ArgValue::String("001-school-unit".to_owned())),
                    ("status", ArgValue::String("published".to_owned())),
                ]),
                &ctx(dir.path()),
            )
            .await;
        assert!(archived.is_success(), "{:?}", archived.error());

        let release = dir.path().join("releases/001-school-unit");
        assert_eq!(
            std::fs::read(release.join("film.mp4")).expect("the film"),
            b"film"
        );
        assert_eq!(
            std::fs::read(release.join("subtitles.srt")).expect("the subtitles"),
            b"subs"
        );
        assert!(release.join("meta.json").is_file());
        assert!(!episode.join("out/frames").exists());

        let series: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(SERIES_INDEX)).expect("the index"),
        )
        .expect("the index parses");
        assert_eq!(series["episodes"][0]["status"], "published");
        assert_eq!(series["episodes"][0]["release"], "releases/001-school-unit");
    }

    #[tokio::test]
    async fn a_slug_must_be_lowercase_ascii() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let result = NewEpisodeTool::new()
            .call(
                args(&[
                    ("slug", ArgValue::String("学校".to_owned())),
                    ("title", ArgValue::String("学校单元".to_owned())),
                ]),
                &ctx(dir.path()),
            )
            .await;
        assert!(!result.is_success());
    }
}
