//! Screen tools: enumerate windows and hand a screenshot to the model.
//!
//! The tools do no recognition. `list_windows` names what can be captured and
//! `screen_capture` returns pixels; the model's vision is what reads them.
//! Captures are written to a per-conversation folder outside the workspace,
//! and the folder is the permission scope: `workspace_auto` asks once, an
//! "allow this path for the session" grant silences later calls, and
//! `full_auto` never asks.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use keel_core::{
    AccessKind, ArgValue, ImagePart, ParamKind, ParamSpec, PermissionRequest, RiskLevel, Tool,
    ToolArgs, ToolCtx, ToolError, ToolResult, ToolSchema,
};

/// The provider resizes a larger image anyway; this keeps the request small.
const MAX_SIDE: u32 = 1568;
/// A PNG above this size is refused instead of inflating the request.
const MAX_IMAGE_BYTES: usize = 6 * 1024 * 1024;

/// Lists the windows the user can ask to capture.
pub struct ListWindowsTool;

impl ListWindowsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ListWindowsTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "list_windows".to_owned(),
            description: "List visible windows on this machine: id, application, title and \
                          size. Use an id (or a distinctive title) with screen_capture."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: Vec::new(),
        }
    }

    fn permission_requests(&self, _args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        Ok(Vec::new())
    }

    async fn call(&self, _args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        match list_windows() {
            Ok(text) => ToolResult::success(ctx.call_id, "list_windows", text),
            Err(error) => ToolResult::failure(ctx.call_id, "list_windows", ToolError::io(error)),
        }
    }
}

/// Captures a window, a screen region or a whole monitor.
pub struct ScreenCaptureTool {
    screens: PathBuf,
}

impl ScreenCaptureTool {
    pub fn new(screens: PathBuf) -> Self {
        Self { screens }
    }
}

#[async_trait]
impl Tool for ScreenCaptureTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "screen_capture".to_owned(),
            description: "Capture a screenshot and attach it to the result. `target` is \
                          `window` (with `window` as an id from list_windows or a title \
                          fragment), `region` (with `region` as \"x,y,w,h\") or `screen` \
                          (with an optional `monitor` index). Quote what the image shows; \
                          do not describe text you cannot see."
                .to_owned(),
            risk: RiskLevel::ReadOnly,
            params: vec![
                ParamSpec {
                    name: "target".to_owned(),
                    kind: ParamKind::String,
                    required: true,
                },
                ParamSpec {
                    name: "window".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
                ParamSpec {
                    name: "region".to_owned(),
                    kind: ParamKind::String,
                    required: false,
                },
                ParamSpec {
                    name: "monitor".to_owned(),
                    kind: ParamKind::Integer,
                    required: false,
                },
            ],
        }
    }

    fn permission_requests(&self, args: &ToolArgs) -> Result<Vec<PermissionRequest>, ToolError> {
        // The folder is outside the workspace and its roots, so
        // `workspace_auto` confirms and `full_auto` does not. The folder is
        // also the grant scope for "allow this path for the session".
        let detail = match args.require_str("target")? {
            "window" => format!(
                "window {}",
                args.get("window")
                    .and_then(|value| match value {
                        ArgValue::String(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .unwrap_or("?")
            ),
            "region" => "screen region".to_owned(),
            _ => "the screen".to_owned(),
        };
        // The tool writes the screenshot into the folder, so the request is a
        // write scoped to that folder. Outside every root, `workspace_auto`
        // confirms it and a session grant covers later captures.
        Ok(vec![PermissionRequest {
            tool: "screen_capture".to_owned(),
            access: AccessKind::Write,
            path: Some(self.screens.clone()),
            detail: Some(detail),
        }])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let target = match args.require_str("target") {
            Ok(target) => target.trim().to_lowercase(),
            Err(error) => return ToolResult::failure(ctx.call_id, "screen_capture", error),
        };
        let captured = match target.as_str() {
            "window" => {
                let wanted = match args.require_str("window") {
                    Ok(wanted) => wanted,
                    Err(error) => {
                        return ToolResult::failure(ctx.call_id, "screen_capture", error);
                    }
                };
                capture_window(wanted)
            }
            "region" => {
                let region = match args.require_str("region") {
                    Ok(region) => region,
                    Err(error) => {
                        return ToolResult::failure(ctx.call_id, "screen_capture", error);
                    }
                };
                parse_region(region).and_then(capture_region)
            }
            "screen" => {
                let monitor = match args.get("monitor") {
                    Some(ArgValue::Integer(value)) => (*value).max(0) as usize,
                    Some(_) => {
                        return ToolResult::failure(
                            ctx.call_id,
                            "screen_capture",
                            ToolError::invalid_arguments("`monitor` must be an integer"),
                        );
                    }
                    None => 0,
                };
                capture_screen(monitor)
            }
            other => {
                return ToolResult::failure(
                    ctx.call_id,
                    "screen_capture",
                    ToolError::invalid_arguments(format!(
                        "`target` must be `window`, `region` or `screen`, not `{other}`"
                    )),
                );
            }
        };
        let (png, width, height, label) = match captured {
            Ok(captured) => captured,
            Err(error) => {
                return ToolResult::failure(ctx.call_id, "screen_capture", ToolError::io(error));
            }
        };
        if png.len() > MAX_IMAGE_BYTES {
            return ToolResult::failure(
                ctx.call_id,
                "screen_capture",
                ToolError::io(format!(
                    "the screenshot is {} bytes, over the {} limit",
                    png.len(),
                    MAX_IMAGE_BYTES
                )),
            );
        }
        if let Err(error) = std::fs::create_dir_all(&self.screens) {
            return ToolResult::failure(
                ctx.call_id,
                "screen_capture",
                ToolError::io(format!("cannot create {}: {error}", self.screens.display())),
            );
        }
        let path = self.screens.join(format!("screen-{}.png", now_ms()));
        if let Err(error) = std::fs::write(&path, &png) {
            return ToolResult::failure(
                ctx.call_id,
                "screen_capture",
                ToolError::io(format!("cannot write {}: {error}", path.display())),
            );
        }
        ToolResult::success(
            ctx.call_id,
            "screen_capture",
            format!(
                "captured {label} ({width}x{height}); saved at {}",
                path.display()
            ),
        )
        .with_images(vec![ImagePart::png(png)])
    }
}

/// Opens a saved image and attaches it, so a later turn can look at a
/// screenshot again or see an attached picture.
pub struct ViewImageTool;

impl ViewImageTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ViewImageTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "view_image".to_owned(),
            description: "Read an image file (PNG, JPEG, WebP or GIF) and attach it so you can \
                          look at it. Use it to re-open a screenshot saved earlier or an image \
                          the user attached, by path."
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
            tool: "view_image".to_owned(),
            access: AccessKind::Read,
            path: Some(PathBuf::from(path)),
            detail: None,
        }])
    }

    async fn call(&self, args: ToolArgs, ctx: &ToolCtx) -> ToolResult {
        let path = match args.require_str("path") {
            Ok(path) => path,
            Err(error) => return ToolResult::failure(ctx.call_id, "view_image", error),
        };
        let resolved = keel_core::resolve_path(&ctx.workspace_root, Path::new(path));
        match read_image(&resolved) {
            Ok((data, media_type, width, height)) => ToolResult::success(
                ctx.call_id,
                "view_image",
                format!(
                    "viewed {} ({width}x{height}, {media_type})",
                    resolved.display()
                ),
            )
            .with_images(vec![ImagePart::new(media_type, data)]),
            Err(error) => ToolResult::failure(ctx.call_id, "view_image", ToolError::io(error)),
        }
    }
}

fn read_image(path: &Path) -> Result<(Vec<u8>, String, u32, u32), String> {
    let data =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if data.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "{} is {} bytes, over the {} limit",
            path.display(),
            data.len(),
            MAX_IMAGE_BYTES
        ));
    }
    let format =
        image::guess_format(&data).map_err(|_| format!("{} is not an image", path.display()))?;
    let media_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Gif => "image/gif",
        other => {
            return Err(format!(
                "{} is {other:?}, which is not shown",
                path.display()
            ))
        }
    };
    let decoded = image::load_from_memory_with_format(&data, format)
        .map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
    let (width, height) = (decoded.width(), decoded.height());
    if width.max(height) > MAX_SIDE {
        // The provider would resize it anyway; do it once here.
        let scaled = decoded.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
        let (png, _, _) = encode_png(scaled.to_rgba8())?;
        return Ok((png, "image/png".to_owned(), width, height));
    }
    Ok((data, media_type.to_owned(), width, height))
}

fn list_windows() -> Result<String, String> {
    let windows = xcap::Window::all().map_err(|error| format!("cannot list windows: {error}"))?;
    let mut lines = Vec::new();
    for window in windows.into_iter().take(40) {
        let title = window.title().unwrap_or_default();
        let minimized = window.is_minimized().unwrap_or(false);
        if title.trim().is_empty() || minimized {
            continue;
        }
        lines.push(format!(
            "id={} app={} title={} size={}x{}",
            window.id().unwrap_or(0),
            window.app_name().unwrap_or_default(),
            title,
            window.width().unwrap_or(0),
            window.height().unwrap_or(0)
        ));
    }
    if lines.is_empty() {
        lines.push("no visible windows".to_owned());
    }
    // Monitors are listed after the windows so `screen_capture` with
    // `target: "screen"` has an index to use.
    if let Ok(monitors) = xcap::Monitor::all() {
        for (index, monitor) in monitors.into_iter().enumerate() {
            lines.push(format!(
                "monitor {index}: {} {}x{} at {},{}",
                monitor.name().unwrap_or_default(),
                monitor.width().unwrap_or(0),
                monitor.height().unwrap_or(0),
                monitor.x().unwrap_or(0),
                monitor.y().unwrap_or(0)
            ));
        }
    }
    Ok(lines.join("\n"))
}

fn capture_window(wanted: &str) -> Result<(Vec<u8>, u32, u32, String), String> {
    let windows = xcap::Window::all().map_err(|error| format!("cannot list windows: {error}"))?;
    let wanted_id = wanted.parse::<u32>().ok();
    let wanted_lower = wanted.to_lowercase();
    let window = windows
        .into_iter()
        .find(|window| {
            let id_matches = wanted_id
                .map(|id| window.id().map(|actual| actual == id).unwrap_or(false))
                .unwrap_or(false);
            let title_matches = window
                .title()
                .map(|title| title.to_lowercase().contains(&wanted_lower))
                .unwrap_or(false);
            id_matches || title_matches
        })
        .ok_or_else(|| format!("no visible window matches `{wanted}`"))?;
    let title = window.title().unwrap_or_default();
    let image = window
        .capture_image()
        .map_err(|error| format!("cannot capture `{title}`: {error}"))?;
    let (png, width, height) = encode_png(image)?;
    Ok((png, width, height, format!("window \"{title}\"")))
}

fn parse_region(text: &str) -> Result<(i32, i32, u32, u32), String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err("`region` must be \"x,y,w,h\"".to_owned());
    }
    let value = |index: usize| -> Result<i32, String> {
        parts[index]
            .parse::<i32>()
            .map_err(|_| format!("`region` part `{}` is not a number", parts[index]))
    };
    let x = value(0)?;
    let y = value(1)?;
    let width =
        u32::try_from(value(2)?).map_err(|_| "`region` width must be positive".to_owned())?;
    let height =
        u32::try_from(value(3)?).map_err(|_| "`region` height must be positive".to_owned())?;
    if width == 0 || height == 0 {
        return Err("`region` width and height must be positive".to_owned());
    }
    Ok((x, y, width, height))
}

fn capture_region(
    (x, y, width, height): (i32, i32, u32, u32),
) -> Result<(Vec<u8>, u32, u32, String), String> {
    let monitors =
        xcap::Monitor::all().map_err(|error| format!("cannot list monitors: {error}"))?;
    let monitor = monitors
        .into_iter()
        .find(|monitor| {
            let left = monitor.x().unwrap_or(0);
            let top = monitor.y().unwrap_or(0);
            let right = left.saturating_add(monitor.width().unwrap_or(0) as i32);
            let bottom = top.saturating_add(monitor.height().unwrap_or(0) as i32);
            x >= left && y >= top && x < right && y < bottom
        })
        .ok_or_else(|| format!("the region starts at {x},{y}, which no monitor contains"))?;
    let left = monitor.x().unwrap_or(0);
    let top = monitor.y().unwrap_or(0);
    let image = monitor
        .capture_image()
        .map_err(|error| format!("cannot capture the monitor: {error}"))?;
    let local_x = u32::try_from(x - left).unwrap_or(0);
    let local_y = u32::try_from(y - top).unwrap_or(0);
    let crop_width = width.min(image.width().saturating_sub(local_x));
    let crop_height = height.min(image.height().saturating_sub(local_y));
    if crop_width == 0 || crop_height == 0 {
        return Err("the region lies outside the monitor".to_owned());
    }
    let cropped =
        image::imageops::crop_imm(&image, local_x, local_y, crop_width, crop_height).to_image();
    let (png, width, height) = encode_png(cropped)?;
    Ok((
        png,
        width,
        height,
        format!("region {x},{y} {width}x{height}"),
    ))
}

fn capture_screen(index: usize) -> Result<(Vec<u8>, u32, u32, String), String> {
    let monitors =
        xcap::Monitor::all().map_err(|error| format!("cannot list monitors: {error}"))?;
    let monitor = monitors
        .into_iter()
        .nth(index)
        .ok_or_else(|| format!("there is no monitor {index}"))?;
    let name = monitor.name().unwrap_or_default();
    let image = monitor
        .capture_image()
        .map_err(|error| format!("cannot capture monitor {index}: {error}"))?;
    let (png, width, height) = encode_png(image)?;
    Ok((png, width, height, format!("monitor {index} ({name})")))
}

fn encode_png(image: image::RgbaImage) -> Result<(Vec<u8>, u32, u32), String> {
    let (width, height) = image.dimensions();
    let scaled = if width.max(height) > MAX_SIDE {
        let ratio = MAX_SIDE as f32 / width.max(height) as f32;
        let target_width = ((width as f32 * ratio) as u32).max(1);
        let target_height = ((height as f32 * ratio) as u32).max(1);
        image::imageops::resize(
            &image,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(scaled)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|error| format!("cannot encode the screenshot: {error}"))?;
    Ok((cursor.into_inner(), width, height))
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// Where one conversation keeps its screenshots. The folder is the permission
/// scope and is outside the workspace, so a capture is a confirmed read
/// rather than a silent one.
pub fn screens_dir(project_dir: &Path, store_path: &Path) -> PathBuf {
    let session = store_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".to_owned());
    screens_dir_for(project_dir, &session)
}

/// The screenshots of one conversation, by its id.
pub fn screens_dir_for(project_dir: &Path, session_id: &str) -> PathBuf {
    project_dir.join("screens").join(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_must_be_four_positive_numbers() {
        assert_eq!(parse_region("10,20,300,400"), Ok((10, 20, 300, 400)));
        assert!(parse_region("10,20,300").is_err());
        assert!(parse_region("a,b,c,d").is_err());
        assert!(parse_region("0,0,0,10").is_err());
    }

    #[test]
    fn a_capture_asks_for_its_own_folder() {
        let tool = ScreenCaptureTool::new(PathBuf::from("/tmp/screens"));
        let mut args = ToolArgs::new();
        args.insert("target", ArgValue::String("window".to_owned()));
        args.insert("window", ArgValue::String("Notepad".to_owned()));

        let requests = tool.permission_requests(&args).expect("a request");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].access, AccessKind::Write);
        assert_eq!(requests[0].path.as_deref(), Some(Path::new("/tmp/screens")));
        assert_eq!(requests[0].detail.as_deref(), Some("window Notepad"));
    }

    #[test]
    #[ignore = "needs an interactive desktop session"]
    fn a_window_can_be_listed_and_captured() {
        let listing = list_windows().expect("windows");
        let first = listing.lines().next().expect("at least one visible window");
        let id = first
            .split("id=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .expect("an id");

        let (png, width, height, label) = capture_window(id).expect("the capture");

        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "{label}");
        assert!(width > 0 && height > 0, "{label}");
    }

    #[test]
    #[ignore = "needs an interactive desktop session"]
    fn a_monitor_can_be_captured() {
        let (png, width, height, label) = capture_screen(0).expect("the capture");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "{label}");
        assert!(width > 0 && height > 0, "{label}");
    }

    #[test]
    fn a_large_capture_is_scaled_and_encoded() {
        let image = image::RgbaImage::from_pixel(2000, 1000, image::Rgba([1, 2, 3, 255]));

        let (png, width, height) = encode_png(image).expect("the png");

        assert_eq!((width, height), (2000, 1000));
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let decoded = image::load_from_memory(&png).expect("the png decodes");
        assert_eq!(decoded.width(), MAX_SIDE);
        assert!(decoded.height() <= MAX_SIDE);
    }
}
