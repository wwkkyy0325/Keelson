//! Video-production tools: render an HTML scene to a PNG sequence and probe a
//! media file.
//!
//! `render_scene` serves the project over loopback, opens it in a headless
//! browser and receives one PNG per frame from the page. The scene decides
//! what to draw; this tool only turns time into files, which is what makes a
//! frame checkable before any video exists.
//!
//! `probe_media` reports what a finished file is: duration, streams and the
//! video geometry. The model reads frames with `view_image`; neither tool
//! inspects content.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use keel_core::{
    AccessKind, ArgValue, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool, ToolArgs,
    ToolCtx, ToolError, ToolResult, ToolSchema,
};

/// Header bytes accepted before a request is refused.
const MAX_HEADER_BYTES: usize = 16 * 1024;
/// A single PNG above this size is refused instead of filling the disk.
const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;
/// Rendering longer than this is a mistake, not a film.
const MAX_DURATION_MS: u64 = 10 * 60 * 1000;

/// Renders one scene to `out/<index>.png` by asking the page for each frame.
pub struct RenderSceneTool;

impl RenderSceneTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for RenderSceneTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "render_scene".to_owned(),
            description: "Render a scene from the project's kit to a PNG sequence. `root` is \
                          the project directory and must contain `kit/scene.html` and \
                          `scenes/<scene>.js`. The page draws a frame for every t and posts it \
                          back; the tool writes 0000.png, 0001.png and so on. Look at a frame \
                          with view_image before assembling anything."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![
                ParamSpec {
                    name: "root".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "scene".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "duration_ms".to_owned(),
                    kind: ParamKind::Integer,
                    required: true,
                },
                ParamSpec {
                    name: "fps".to_owned(),
                    kind: ParamKind::Integer,
                    required: false,
                },
                ParamSpec {
                    name: "page".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
                ParamSpec {
                    name: "out".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        let root = PathBuf::from(args.require_str("root")?);
        let scene = args.require_str("scene")?.to_owned();
        page_path(args)?;
        let out = frame_dir(&root, args)?;
        Ok(vec![PermissionRequest {
            tool: "render_scene".to_owned(),
            access: AccessKind::Write,
            path: Some(out),
            detail: Some(format!("scene {scene}")),
        }])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let root = match args.require_str("root") {
            Ok(root) => PathBuf::from(root),
            Err(error) => return ToolResult::failure(ctx.call_id, "render_scene", error),
        };
        let root = keel_core::resolve_path(&ctx.workspace_root, &root);
        let scene = match args.require_str("scene") {
            Ok(scene) => scene.trim().to_owned(),
            Err(error) => return ToolResult::failure(ctx.call_id, "render_scene", error),
        };
        if scene.is_empty()
            || !scene
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return ToolResult::failure(
                ctx.call_id,
                "render_scene",
                ToolError::invalid_arguments("`scene` must be a plain scene name, like `01`"),
            );
        }
        let duration_ms = match integer(&args, "duration_ms", None) {
            Ok(value) if value > 0 && value as u64 <= MAX_DURATION_MS => value as u64,
            Ok(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "render_scene",
                    ToolError::invalid_arguments(format!(
                        "`duration_ms` must be between 1 and {MAX_DURATION_MS}"
                    )),
                )
            }
            Err(error) => return ToolResult::failure(ctx.call_id, "render_scene", error),
        };
        let fps = match integer(&args, "fps", Some(30)) {
            Ok(value) if (1..=60).contains(&value) => value as u32,
            Ok(_) => {
                return ToolResult::failure(
                    ctx.call_id,
                    "render_scene",
                    ToolError::invalid_arguments("`fps` must be between 1 and 60"),
                )
            }
            Err(error) => return ToolResult::failure(ctx.call_id, "render_scene", error),
        };
        let page = match page_path(&args) {
            Ok(page) => page,
            Err(error) => return ToolResult::failure(ctx.call_id, "render_scene", error),
        };
        let out = match frame_dir(&root, &args) {
            Ok(out) => out,
            Err(error) => return ToolResult::failure(ctx.call_id, "render_scene", error),
        };

        let rendered = tokio::task::spawn_blocking(move || {
            render(&root, &page, &scene, duration_ms, fps, &out)
        })
        .await;
        match rendered {
            Ok(Ok(text)) => ToolResult::success(ctx.call_id, "render_scene", text),
            Ok(Err(error)) => {
                ToolResult::failure(ctx.call_id, "render_scene", ToolError::io(error))
            }
            Err(error) => ToolResult::failure(
                ctx.call_id,
                "render_scene",
                ToolError::io(format!("the renderer did not finish: {error}")),
            ),
        }
    }
}

/// Reports what a media file contains, so a finished film can be checked.
pub struct ProbeMediaTool;

impl ProbeMediaTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ProbeMediaTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "probe_media".to_owned(),
            description: "Report a media file's duration, size, video geometry and audio \
                          streams. Use it after assembling a film: the duration must match the \
                          timeline, the video must be the size the storyboard asked for, and \
                          there must be an audio stream."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![ParamSpec {
                name: "path".to_owned(),
                kind: ParamKind::String,
                required: true,
            }],
        }
    }

    fn permission_requests(&self, args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        let path = args.require_str("path")?;
        Ok(vec![PermissionRequest {
            tool: "probe_media".to_owned(),
            access: AccessKind::Read,
            path: Some(PathBuf::from(path)),
            detail: None,
        }])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let path = match args.require_str("path") {
            Ok(path) => path.to_owned(),
            Err(error) => return ToolResult::failure(ctx.call_id, "probe_media", error),
        };
        let resolved = keel_core::resolve_path(&ctx.workspace_root, Path::new(&path));
        let summary = tokio::task::spawn_blocking(move || probe(&resolved)).await;
        match summary {
            Ok(Ok(text)) => ToolResult::success(ctx.call_id, "probe_media", text),
            Ok(Err(error)) => ToolResult::failure(ctx.call_id, "probe_media", ToolError::io(error)),
            Err(error) => ToolResult::failure(
                ctx.call_id,
                "probe_media",
                ToolError::io(format!("the probe did not finish: {error}")),
            ),
        }
    }
}

fn integer(args: &ToolArgs, name: &str, default: Option<i64>) -> Result<i64, ToolError> {
    match args.get(name) {
        Some(ArgValue::Integer(value)) => Ok(*value),
        Some(other) => Err(ToolError::invalid_arguments(format!(
            "`{name}` must be an integer, got {}",
            other.kind().as_str()
        ))),
        None => {
            default.ok_or_else(|| ToolError::invalid_arguments(format!("`{name}` is required")))
        }
    }
}

fn page_path(args: &ToolArgs) -> Result<String, ToolError> {
    let page = match args.get("page") {
        Some(ArgValue::String(page)) if !page.trim().is_empty() => page.trim().to_owned(),
        Some(ArgValue::String(_)) => {
            return Err(ToolError::invalid_arguments("`page` is empty"));
        }
        Some(other) => {
            return Err(ToolError::invalid_arguments(format!(
                "`page` must be a string, got {}",
                other.kind().as_str()
            )));
        }
        None => "kit/scene.html".to_owned(),
    };
    let path = Path::new(&page);
    if path.is_absolute()
        || page.contains("..")
        || !page.starts_with("kit/")
        || !page.ends_with(".html")
    {
        return Err(ToolError::invalid_arguments(
            "`page` must be an HTML file under `kit/`, like `kit/scene3d.html`",
        ));
    }
    Ok(page)
}

fn frame_dir(root: &Path, args: &ToolArgs) -> Result<PathBuf, ToolError> {
    let scene = args.require_str("scene")?;
    match args.get("out") {
        Some(ArgValue::String(out)) if !out.trim().is_empty() => Ok(PathBuf::from(out)),
        Some(ArgValue::String(_)) => Err(ToolError::invalid_arguments("`out` is empty")),
        Some(other) => Err(ToolError::invalid_arguments(format!(
            "`out` must be a string, got {}",
            other.kind().as_str()
        ))),
        None => Ok(root.join("out").join("frames").join(scene)),
    }
}

#[derive(Default)]
struct SinkState {
    frames: usize,
    done: bool,
    error: Option<String>,
}

fn render(
    root: &Path,
    page: &str,
    scene: &str,
    duration_ms: u64,
    fps: u32,
    out: &Path,
) -> Result<String, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot open {}: {error}", root.display()))?;
    let scene_html = root.join(page);
    if !scene_html.is_file() {
        return Err(format!(
            "{} is missing; the project needs the explainer kit",
            scene_html.display()
        ));
    }
    let scene_js = root.join("scenes").join(format!("{scene}.js"));
    if !scene_js.is_file() {
        return Err(format!("{} is missing", scene_js.display()));
    }
    std::fs::create_dir_all(out)
        .map_err(|error| format!("cannot create {}: {error}", out.display()))?;
    // A shorter re-render must not leave frames of the previous take behind.
    if let Ok(entries) = std::fs::read_dir(out) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("png") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let expected = ((duration_ms as f64 / 1000.0) * f64::from(fps))
        .round()
        .max(1.0) as usize;
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("cannot listen on loopback: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("cannot read the listener address: {error}"))?
        .port();
    let state = Arc::new(Mutex::new(SinkState::default()));
    let stop = Arc::new(AtomicBool::new(false));
    {
        let state = state.clone();
        let stop = stop.clone();
        let root = root.clone();
        let out = out.to_path_buf();
        std::thread::spawn(move || serve(listener, root, out, state, stop));
    }

    let profile =
        std::env::temp_dir().join(format!("keelson-edge-{}-{}", std::process::id(), now_ms()));
    let url = format!(
        "http://127.0.0.1:{port}/{page}?scene={scene}&duration={:.3}&fps={fps}",
        duration_ms as f64 / 1000.0
    );
    let mut child = launch_browser(&profile, &url)?;
    let deadline = Instant::now()
        + Duration::from_millis(60_000)
        + Duration::from_millis(duration_ms.saturating_mul(10)).min(Duration::from_secs(840));

    let mut failure = None;
    loop {
        if Instant::now() >= deadline {
            failure = Some(format!(
                "the renderer timed out after {:.0}s",
                (60_000 + duration_ms.saturating_mul(10)) as f64 / 1000.0
            ));
            break;
        }
        if let Ok(child_state) = child.try_wait() {
            if child_state.is_some() {
                failure = Some("the browser exited before the scene finished".to_owned());
                break;
            }
        }
        let snapshot = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(error) = snapshot.error.clone() {
            failure = Some(error);
            break;
        }
        if snapshot.done {
            break;
        }
        drop(snapshot);
        std::thread::sleep(Duration::from_millis(20));
    }

    stop.store(true, Ordering::Relaxed);
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&profile);

    let frames = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .frames;
    if let Some(error) = failure {
        return Err(format!("{error}; {frames} frame(s) were written"));
    }
    if frames == 0 {
        return Err(
            "the scene produced no frames; check that kit/scene.html posts each frame".to_owned(),
        );
    }
    if frames < expected {
        return Err(format!(
            "the scene produced {frames} of {expected} frames; check the page for an error"
        ));
    }
    Ok(format!(
        "rendered {frames} frames at {fps} fps to {}",
        out.display()
    ))
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0)
}

fn launch_browser(profile: &Path, url: &str) -> Result<Child, String> {
    let mut last = String::new();
    for candidate in browser_candidates() {
        let mut command = Command::new(&candidate);
        command
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--hide-scrollbars")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg("--window-size=1920,1080")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => last = format!("{}: {error}", candidate.display()),
        }
    }
    Err(format!(
        "cannot start a browser for rendering ({last}); install Edge or Chrome, or set KEELSON_EDGE"
    ))
}

fn browser_candidates() -> Vec<PathBuf> {
    if let Ok(path) = std::env::var("KEELSON_EDGE") {
        if !path.trim().is_empty() {
            return vec![PathBuf::from(path)];
        }
    }
    let mut candidates = Vec::new();
    if cfg!(windows) {
        candidates.push(PathBuf::from(
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ));
        candidates.push(PathBuf::from(
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ));
        candidates.push(PathBuf::from(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        ));
        candidates.push(PathBuf::from(
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ));
    }
    candidates.push(PathBuf::from("msedge"));
    candidates.push(PathBuf::from("google-chrome"));
    candidates.push(PathBuf::from("chromium"));
    candidates
}

fn serve(
    listener: TcpListener,
    root: PathBuf,
    out: PathBuf,
    state: Arc<Mutex<SinkState>>,
    stop: Arc<AtomicBool>,
) {
    let _ = listener.set_nonblocking(true);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let root = root.clone();
                let out = out.clone();
                let state = state.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle(stream, &root, &out, &state) {
                        let mut shared = state
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        if shared.error.is_none() {
                            shared.error = Some(error);
                        }
                    }
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

fn handle(
    stream: TcpStream,
    root: &Path,
    out: &Path,
    state: &Arc<Mutex<SinkState>>,
) -> Result<(), String> {
    // A socket accepted from a non-blocking listener inherits that mode on
    // Windows, so a plain read would fail with `WouldBlock`.
    stream
        .set_nonblocking(false)
        .and_then(|_| stream.set_read_timeout(Some(Duration::from_secs(15))))
        .map_err(|error| format!("cannot configure the connection: {error}"))?;
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    if let Err(error) = reader.read_line(&mut request) {
        if benign(&error) {
            return Ok(());
        }
        return Err(format!("cannot read the request: {error}"));
    }
    if request.trim().is_empty() {
        return Ok(());
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();

    let mut content_length = 0usize;
    let mut header_bytes = request.len();
    loop {
        let mut line = String::new();
        let read = match reader.read_line(&mut line) {
            Ok(read) => read,
            Err(error) if benign(&error) => return Ok(()),
            Err(error) => return Err(format!("cannot read a header: {error}")),
        };
        if read == 0 || line == "\r\n" {
            break;
        }
        header_bytes += read;
        if header_bytes > MAX_HEADER_BYTES {
            return Err("the request headers are too large".to_owned());
        }
        if let Some(value) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(str::trim)
            .map(str::to_owned)
        {
            content_length = value
                .parse()
                .map_err(|_| format!("invalid content length `{value}`"))?;
        }
    }

    let path = target.split('?').next().unwrap_or("/");
    let query = target.split_once('?').map(|(_, query)| query).unwrap_or("");

    if method == "POST" && path == "/done" {
        respond(&stream, "200 OK", "text/plain", b"ok")?;
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .done = true;
        return Ok(());
    }

    if method == "POST" && path == "/frame" {
        if content_length == 0 || content_length > MAX_FRAME_BYTES {
            respond(&stream, "413 Payload Too Large", "text/plain", b"no")?;
            return Err(format!("a frame is {content_length} bytes"));
        }
        let mut body = vec![0u8; content_length];
        if let Err(error) = reader.read_exact(&mut body) {
            if benign(&error) {
                return Ok(());
            }
            return Err(format!("cannot read the frame: {error}"));
        }
        let text = String::from_utf8_lossy(&body);
        let encoded = text
            .strip_prefix("data:image/png;base64,")
            .ok_or_else(|| "a frame is not a PNG data URL".to_owned())?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| format!("cannot decode a frame: {error}"))?;
        let index = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("index="))
            .unwrap_or("0000");
        if !index.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("invalid frame index `{index}`"));
        }
        let path = out.join(format!("{index}.png"));
        std::fs::write(&path, png)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frames += 1;
        respond(&stream, "200 OK", "text/plain", b"ok")?;
        return Ok(());
    }

    if method == "GET" {
        let Some(file) = static_path(root, path) else {
            respond(&stream, "404 Not Found", "text/plain", b"not found")?;
            return Ok(());
        };
        let body = std::fs::read(&file)
            .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
        let content_type = content_type_for(&file);
        respond(&stream, "200 OK", content_type, &body)?;
        return Ok(());
    }

    respond(&stream, "404 Not Found", "text/plain", b"not found")?;
    Ok(())
}

fn respond(
    mut stream: &TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    match stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(body))
        .and_then(|_| stream.flush())
    {
        Ok(()) => Ok(()),
        // A speculative connection the browser has already dropped is not a
        // render failure; a missing frame is caught by the frame count.
        Err(error) if benign(&error) => Ok(()),
        Err(error) => Err(format!("cannot answer the browser: {error}")),
    }
}

fn benign(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
    )
}

fn static_path(root: &Path, request: &str) -> Option<PathBuf> {
    let decoded = percent_decode(request);
    let relative = decoded.trim_start_matches('/');
    if relative.is_empty() {
        return None;
    }
    let candidate = root.join(relative).canonicalize().ok()?;
    if !candidate.starts_with(root) || !candidate.is_file() {
        return None;
    }
    Some(candidate)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn probe(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!("{} does not exist", path.display()));
    }
    let program = std::env::var("KEELSON_FFPROBE").unwrap_or_else(|_| "ffprobe".to_owned());
    let output = Command::new(&program)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration,size",
            "-show_entries",
            "stream=codec_type,codec_name,width,height,r_frame_rate,sample_rate,channels",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run {program}: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{program} refused {}: {}",
            path.display(),
            detail.trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut summary = summarize_probe(&text)?;
    summary.insert_str(0, &format!("file: {}\n", path.display()));
    Ok(summary)
}

fn summarize_probe(json: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("ffprobe returned no JSON: {error}"))?;
    let duration = value
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(serde_json::Value::as_str)
        .and_then(|text| text.parse::<f64>().ok());
    let size = value
        .get("format")
        .and_then(|format| format.get("size"))
        .and_then(serde_json::Value::as_str)
        .and_then(|text| text.parse::<u64>().ok());
    let streams = value
        .get("streams")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut lines = Vec::new();
    if let Some(duration) = duration {
        lines.push(format!("duration: {duration:.3}s"));
    }
    if let Some(size) = size {
        lines.push(format!("size: {size} bytes"));
    }
    match streams.iter().find(|stream| {
        stream.get("codec_type").and_then(serde_json::Value::as_str) == Some("video")
    }) {
        Some(video) => {
            let codec = video
                .get("codec_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let width = video
                .get("width")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let height = video
                .get("height")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let fps = video
                .get("r_frame_rate")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            lines.push(format!("video: {codec} {width}x{height} {fps} fps"));
        }
        None => lines.push("video: none".to_owned()),
    }
    let audio: Vec<String> = streams
        .iter()
        .filter(|stream| {
            stream.get("codec_type").and_then(serde_json::Value::as_str) == Some("audio")
        })
        .map(|stream| {
            let codec = stream
                .get("codec_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let rate = stream
                .get("sample_rate")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let channels = stream
                .get("channels")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            format!("{codec} {rate} Hz {channels} ch")
        })
        .collect();
    if audio.is_empty() {
        lines.push("audio: none".to_owned());
    } else {
        lines.push(format!("audio: {}", audio.join("; ")));
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(workspace: &Path) -> ToolCtx {
        ToolCtx {
            turn_id: keel_core::TurnId::new(1),
            call_id: keel_core::ToolCallId::new(1),
            workspace_root: workspace.to_path_buf(),
            cancel: keel_core::CancelToken::new(),
        }
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("the target dir");
        for entry in std::fs::read_dir(from).expect("a readable dir").flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).expect("a copied file");
            }
        }
    }

    #[test]
    fn a_page_must_stay_under_the_kit() {
        let args = ToolArgs::new();
        assert_eq!(
            page_path(&args).expect("the default page"),
            "kit/scene.html"
        );

        let mut args = ToolArgs::new();
        args.insert("page", ArgValue::String("kit/scene3d.html".to_owned()));
        assert_eq!(page_path(&args).expect("a 3D page"), "kit/scene3d.html");

        for bad in [
            "../scene.html",
            "/kit/scene.html",
            "kit/../secret.html",
            "scenes/01.html",
            "kit/scene.js",
        ] {
            let mut args = ToolArgs::new();
            args.insert("page", ArgValue::String(bad.to_owned()));
            assert!(page_path(&args).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn a_static_request_cannot_leave_the_root() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("kit")).expect("the kit dir");
        std::fs::write(root.join("kit/scene.html"), "<html></html>").expect("a file");
        std::fs::write(temp.path().join("secret.txt"), "no").expect("a secret");
        let root = root.canonicalize().expect("a canonical root");

        assert!(static_path(&root, "/kit/scene.html").is_some());
        assert!(static_path(&root, "/../secret.txt").is_none());
        assert!(static_path(&root, "/kit/../../secret.txt").is_none());
        assert!(static_path(&root, "/missing.js").is_none());
    }

    #[test]
    fn a_probe_is_summarized() {
        let json = r#"{
            "format": { "duration": "55.472000", "size": "2671699" },
            "streams": [
                { "codec_type": "video", "codec_name": "h264", "width": 1920,
                  "height": 1080, "r_frame_rate": "30/1" },
                { "codec_type": "audio", "codec_name": "aac",
                  "sample_rate": "44100", "channels": 1 }
            ]
        }"#;
        let summary = summarize_probe(json).expect("a summary");

        assert!(summary.contains("duration: 55.472s"), "{summary}");
        assert!(
            summary.contains("video: h264 1920x1080 30/1 fps"),
            "{summary}"
        );
        assert!(summary.contains("audio: aac 44100 Hz 1 ch"), "{summary}");
    }

    #[test]
    fn a_probe_without_audio_says_so() {
        let json = r#"{
            "format": { "duration": "1.000000", "size": "10" },
            "streams": [ { "codec_type": "video", "codec_name": "h264",
                           "width": 8, "height": 8, "r_frame_rate": "30/1" } ]
        }"#;
        let summary = summarize_probe(json).expect("a summary");
        assert!(summary.contains("audio: none"), "{summary}");
    }

    #[tokio::test]
    #[ignore = "needs Microsoft Edge and ffmpeg"]
    async fn a_scene_renders_and_a_file_probes() {
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("kit")).expect("the kit dir");
        std::fs::create_dir_all(root.join("scenes")).expect("the scenes dir");
        std::fs::write(
            root.join("kit/scene.html"),
            r##"<!doctype html><meta charset="utf-8">
<canvas id="c" width="320" height="180"></canvas>
<script>
(async function () {
  const q = new URLSearchParams(location.search);
  const duration = parseFloat(q.get("duration"));
  const fps = parseInt(q.get("fps"));
  const scene = q.get("scene");
  const canvas = document.getElementById("c");
  const ctx = canvas.getContext("2d");
  const count = Math.max(1, Math.round(duration * fps));
  for (let i = 0; i < count; i++) {
    ctx.fillStyle = i % 2 ? "#f3ede1" : "#d14b2c";
    ctx.fillRect(0, 0, 320, 180);
    const data = canvas.toDataURL("image/png");
    await fetch("/frame?scene=" + scene + "&index=" + String(i).padStart(4, "0"), {
      method: "POST", body: data });
  }
  await fetch("/done", { method: "POST" });
})();
</script>"##,
        )
        .expect("a scene page");
        std::fs::write(root.join("scenes/01.js"), "window.SCENE = { draw() {} };\n")
            .expect("a scene");

        let context = ctx(temp.path());
        let mut args = ToolArgs::new();
        args.insert("root", ArgValue::String(root.display().to_string()));
        args.insert("scene", ArgValue::String("01".to_owned()));
        args.insert("duration_ms", ArgValue::Integer(1_000));
        args.insert("fps", ArgValue::Integer(30));
        let rendered = RenderSceneTool::new().call(args, &context).await;
        assert!(rendered.is_success(), "{:?}", rendered.error());
        let frames = std::fs::read_dir(root.join("out/frames/01"))
            .expect("the frame dir")
            .flatten()
            .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("png"))
            .count();
        assert_eq!(frames, 30, "one second at 30 fps");

        // A tiny film built from the frames, so the probe has something real.
        let video = root.join("out/test.mp4");
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-framerate",
                "30",
                "-i",
            ])
            .arg(root.join("out/frames/01/%04d.png"))
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(&video)
            .status()
            .expect("ffmpeg runs");
        assert!(status.success(), "ffmpeg built the test film");

        let mut args = ToolArgs::new();
        args.insert("path", ArgValue::String(video.display().to_string()));
        let probed = ProbeMediaTool::new().call(args, &context).await;
        let text = probed.content().expect("a probe summary");
        assert!(
            text.contains("1920x1080") || text.contains("320x180"),
            "{text}"
        );
        assert!(text.contains("duration:"), "{text}");
    }

    #[tokio::test]
    #[ignore = "needs Microsoft Edge"]
    async fn the_shipped_kit_renders_a_3d_scene() {
        let kit = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("kits")
            .join("explainer");
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let root = temp.path().join("project");
        copy_dir(&kit.join("kit"), &root.join("kit"));
        std::fs::create_dir_all(root.join("scenes")).expect("the scenes dir");
        std::fs::copy(
            kit.join("scenes/scene3d-template.js"),
            root.join("scenes/01.js"),
        )
        .expect("the 3D template");

        let context = ctx(temp.path());
        let mut args = ToolArgs::new();
        args.insert("root", ArgValue::String(root.display().to_string()));
        args.insert("scene", ArgValue::String("01".to_owned()));
        args.insert("duration_ms", ArgValue::Integer(500));
        args.insert("fps", ArgValue::Integer(30));
        args.insert("page", ArgValue::String("kit/scene3d.html".to_owned()));
        let rendered = RenderSceneTool::new().call(args, &context).await;
        assert!(rendered.is_success(), "{:?}", rendered.error());

        let folder = root.join("out/frames/01");
        let frames: Vec<PathBuf> = std::fs::read_dir(&folder)
            .expect("the frame dir")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("png"))
            .collect();
        assert_eq!(frames.len(), 15, "half a second at 30 fps");
        let first = std::fs::read(&frames[0]).expect("a frame");
        assert!(first.len() > 1_000, "the 3D frame is not blank");
    }

    #[tokio::test]
    #[ignore = "needs Microsoft Edge"]
    async fn the_shipped_kit_renders_a_scene() {
        let kit = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("kits")
            .join("explainer");
        let temp = tempfile::TempDir::new().expect("a temp dir");
        let root = temp.path().join("project");
        copy_dir(&kit.join("kit"), &root.join("kit"));
        copy_dir(&kit.join("data"), &root.join("data"));
        std::fs::create_dir_all(root.join("scenes")).expect("the scenes dir");
        std::fs::write(
            root.join("scenes/01.js"),
            r##"window.SCENE = {
  id: "01",
  draw(ctx, t) {
    const D = window.DESIGN;
    const C = D.C;
    const M = window.DATA;
    D.bg(ctx);
    D.kicker(ctx, "01 / kit check");
    D.headline(ctx, ["A computed value", String(M.window.value)], D.M, 320, 92);
    D.grid(ctx, 200, 560, 4, 4, 70, (r, c) => ({
      fill: (r * 4 + c) % 3 === 0 ? C.vermilion : C.ink,
      stroke: C.line,
    }));
    D.footer(ctx, "explainer kit");
  },
};
"##,
        )
        .expect("a scene");

        let context = ctx(temp.path());
        let mut args = ToolArgs::new();
        args.insert("root", ArgValue::String(root.display().to_string()));
        args.insert("scene", ArgValue::String("01".to_owned()));
        args.insert("duration_ms", ArgValue::Integer(500));
        args.insert("fps", ArgValue::Integer(30));
        let rendered = RenderSceneTool::new().call(args, &context).await;
        assert!(rendered.is_success(), "{:?}", rendered.error());

        let folder = root.join("out/frames/01");
        let frames: Vec<PathBuf> = std::fs::read_dir(&folder)
            .expect("the frame dir")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("png"))
            .collect();
        assert_eq!(frames.len(), 15, "half a second at 30 fps");
        let first = std::fs::read(&frames[0]).expect("a frame");
        assert!(first.len() > 1_000, "the frame is not blank");
    }
}
