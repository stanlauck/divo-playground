// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounded neutral shot lists to deterministic, printable Typst storyboard sheets.
//!
//! See the package README for JSON v1, layout decisions and asset-root semantics.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum UTF-8 JSON input size (16 MiB).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum shots in one board.
pub const MAX_SHOTS: usize = 10_000;
const MAX_TEXT_BYTES: usize = 16_384;

/// Parsing, validation, filesystem and optional compiler errors.
#[derive(Debug)]
pub enum Error {
    /// The byte or shot budget was exceeded.
    Limit(&'static str),
    /// JSON structure, type or syntax error.
    Json(serde_json::Error),
    /// A supported-model constraint failed at this structural location.
    Invalid { field: String, reason: &'static str },
    /// I/O failure while performing the named operation.
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    /// Typst was not found; rendering source does not require Typst.
    TypstNotFound,
    /// The compiler exited unsuccessfully; diagnostics are preserved.
    Compile { status: Option<i32>, stderr: String },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(limit) => write!(f, "limit exceeded: {limit}"),
            Self::Json(e) => write!(f, "invalid JSON: {e}"),
            Self::Invalid { field, reason } => write!(f, "invalid {field}: {reason}"),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::TypstNotFound => write!(f, "Typst not found; install it or use --typst PATH"),
            Self::Compile { status, stderr } => {
                write!(f, "Typst compile failed ({status:?}): {stderr}")
            }
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
fn invalid(field: impl Into<String>, reason: &'static str) -> Error {
    Error::Invalid {
        field: field.into(),
        reason,
    }
}
fn io(operation: &'static str, source: std::io::Error) -> Error {
    Error::Io { operation, source }
}

/// JSON v1 board. Unknown JSON fields are ignored, including nested extensions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Board {
    pub version: u32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub frame_rate: Option<FrameRate>,
    #[serde(default)]
    pub aspect: Aspect,
    pub shots: Vec<Shot>,
}
/// Ordered shot metadata; optional fields accept null.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shot {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub duration_frames: Option<u64>,
    #[serde(default)]
    pub scene: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dialogue: Option<String>,
    #[serde(default)]
    pub camera: Option<String>,
    #[serde(default)]
    pub frame: Option<Frame>,
    #[serde(default)]
    pub notes: Option<String>,
}
/// A local, relative PNG/JPG/SVG reference, never downloaded or converted.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frame {
    pub path: String,
    #[serde(default)]
    pub alt: Option<String>,
}
/// Integer 24/25/30 or a positive rational whose nearest integer is 24/25/30.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum FrameRate {
    Integer(u32),
    Rational { num: u32, den: u32 },
}
impl FrameRate {
    /// Round a rational rate to the nearest nominal integer (half rounds up).
    /// Labels are non-drop and count frames, rather than elapsed wall-clock time.
    pub fn nominal(self) -> Result<u64, Error> {
        let (num, den) = match self {
            Self::Integer(n) => (n, 1),
            Self::Rational { num, den } => (num, den),
        };
        if num == 0 || den == 0 {
            return Err(invalid(
                "frame_rate",
                "numerator and denominator must be positive",
            ));
        }
        let nominal = (u64::from(num) + u64::from(den) / 2) / u64::from(den);
        if ![24, 25, 30].contains(&nominal)
            || matches!(self, Self::Integer(n) if ![24,25,30].contains(&n))
        {
            return Err(invalid("frame_rate", "nominal rate must be 24, 25 or 30"));
        }
        Ok(nominal)
    }
}
/// Frame aspect ratio, independent of paper orientation.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Aspect {
    #[default]
    #[serde(rename = "16:9")]
    Wide,
    #[serde(rename = "4:3")]
    Classic,
    #[serde(rename = "2.39:1")]
    Cinema,
    #[serde(rename = "1:1")]
    Square,
    #[serde(rename = "9:16")]
    Vertical,
}
impl Aspect {
    pub fn ratio(self) -> f64 {
        match self {
            Self::Wide => 16.0 / 9.0,
            Self::Classic => 4.0 / 3.0,
            Self::Cinema => 2.39,
            Self::Square => 1.0,
            Self::Vertical => 9.0 / 16.0,
        }
    }
}
/// Columns × rows per sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grid {
    #[default]
    TwoByThree,
    ThreeByFour,
}
impl Grid {
    pub fn dimensions(self) -> (usize, usize) {
        match self {
            Self::TwoByThree => (2, 3),
            Self::ThreeByFour => (3, 4),
        }
    }
    pub fn cells_per_page(self) -> usize {
        let (c, r) = self.dimensions();
        c * r
    }
    /// Empty boards still produce one printable sheet.
    pub fn page_count(self, shots: usize) -> usize {
        shots.div_ceil(self.cells_per_page()).max(1)
    }
}
/// Supported paper sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    A4,
    Letter,
}
impl Page {
    /// Physical dimensions in PostScript points, before orientation.
    pub fn points(self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
            Self::Letter => (612.0, 792.0),
        }
    }
}
/// Caption fields after the mandatory bold ID, in the caller's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaptionField {
    Name,
    Scene,
    Camera,
    Timecode,
    Description,
    Dialogue,
    Notes,
}
/// Rendering options. Default paper orientation is portrait for both grids.
#[derive(Clone, Debug)]
pub struct Options {
    pub grid: Grid,
    pub page: Page,
    pub landscape: bool,
    pub show_timecodes: bool,
    pub caption_fields: Vec<CaptionField>,
    /// Upper bound in Unicode scalar values per caption (ellipsis included).
    pub max_caption_chars: usize,
    pub title_page: bool,
    pub start_page_number: u32,
    /// Existing frames are resolved here; source image paths are Typst-root-relative.
    pub assets_root: PathBuf,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            grid: Grid::TwoByThree,
            page: Page::A4,
            landscape: false,
            show_timecodes: false,
            caption_fields: vec![
                CaptionField::Name,
                CaptionField::Scene,
                CaptionField::Camera,
                CaptionField::Timecode,
                CaptionField::Description,
                CaptionField::Dialogue,
            ],
            max_caption_chars: 100,
            title_page: false,
            start_page_number: 1,
            assets_root: PathBuf::from("."),
        }
    }
}
/// Fixed cell and frame sizes in points. Margins reserve header/footer space.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub page_width: f64,
    pub page_height: f64,
    pub cell_width: f64,
    pub cell_height: f64,
    pub frame_width: f64,
    pub frame_height: f64,
    pub caption_lines: usize,
}
impl Options {
    /// Compute bounded cell dimensions for all supported paper/aspect combinations.
    pub fn layout(&self, aspect: Aspect) -> Result<Layout, Error> {
        if self.max_caption_chars == 0 || self.max_caption_chars > 1024 {
            return Err(invalid("max_caption_chars", "must be 1..=1024"));
        }
        if self.start_page_number == 0 {
            return Err(invalid("start_page_number", "must be positive"));
        }
        let mut seen = HashSet::new();
        if self.caption_fields.iter().any(|v| !seen.insert(v)) {
            return Err(invalid("caption_fields", "duplicates are not allowed"));
        }
        let (mut w, mut h) = self.page.points();
        if self.landscape {
            std::mem::swap(&mut w, &mut h);
        }
        let (cols, rows) = self.grid.dimensions();
        let cell_width = (w - 28.0 * 72.0 / 25.4 - 8.0 * (cols - 1) as f64) / cols as f64;
        let cell_height = (h - 34.0 * 72.0 / 25.4 - 8.0 * (rows - 1) as f64) / rows as f64;
        let caption_lines = 1 + self.caption_fields.len();
        let inner_width = cell_width - 12.0;
        let available_height = cell_height - 12.0 - caption_lines as f64 * 11.0 - 4.0;
        if available_height < 12.0 {
            return Err(invalid("layout", "insufficient frame space"));
        }
        let frame_height = (inner_width / aspect.ratio()).min(available_height);
        Ok(Layout {
            page_width: w,
            page_height: h,
            cell_width,
            cell_height,
            frame_width: frame_height * aspect.ratio(),
            frame_height,
            caption_lines,
        })
    }
}
/// Nonfatal reason for substituting a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WarningKind {
    NoFrame,
    MissingFile,
    OutsideAssetsRoot,
}
/// Stable shot-indexed warning; no absolute host paths in generated source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub shot_id: String,
    pub kind: WarningKind,
}
/// Complete Typst document plus warnings and unique referenced relative assets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub source: String,
    pub warnings: Vec<Warning>,
    pub assets: Vec<PathBuf>,
}

/// Read at most 16 MiB + one sentinel byte, also for pipes/nonseekable readers.
pub fn read_board(reader: impl Read) -> Result<Board, Error> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| io("read input", e))?;
    from_json(&bytes)
}
/// Strict supported-field parsing and semantic validation; unknown fields ignored.
pub fn from_json(bytes: impl AsRef<[u8]>) -> Result<Board, Error> {
    let bytes = bytes.as_ref();
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(Error::Limit("16 MiB input"));
    }
    let board: Board = serde_json::from_slice(bytes).map_err(Error::Json)?;
    board.validate()?;
    Ok(board)
}
fn validate_text(field: &str, value: &str, max: usize) -> Result<(), Error> {
    if value.len() > max {
        return Err(invalid(field, "text byte limit exceeded"));
    }
    if value.chars().any(|c| {
        (c.is_control() && c != '\n' && c != '\r' && c != '\t')
            || matches!(c, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(invalid(field, "unsupported control character"));
    }
    Ok(())
}
impl Board {
    /// Validate programmatically constructed boards as well as parsed JSON.
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1 {
            return Err(invalid("version", "expected 1"));
        }
        if self.shots.len() > MAX_SHOTS {
            return Err(Error::Limit("10,000 shots"));
        }
        if let Some(r) = self.frame_rate {
            r.nominal()?;
        }
        let mut text_bytes = 0;
        if let Some(title) = &self.title {
            validate_text("title", title, 4096)?;
            text_bytes += title.len();
        }
        let mut ids = HashSet::new();
        for (i, shot) in self.shots.iter().enumerate() {
            let field = format!("shots[{i}]");
            validate_text(&format!("{field}.id"), &shot.id, 256)?;
            if shot.id.is_empty()
                || shot.id.trim() != shot.id
                || shot.id.chars().any(char::is_control)
            {
                return Err(invalid(
                    format!("{field}.id"),
                    "nonempty, trimmed ID required",
                ));
            }
            if !ids.insert(&shot.id) {
                return Err(invalid(format!("{field}.id"), "duplicate ID"));
            }
            text_bytes += shot.id.len();
            if shot.duration_frames == Some(0) {
                return Err(invalid(
                    format!("{field}.duration_frames"),
                    "must be positive",
                ));
            }
            for (name, value) in [
                ("name", &shot.name),
                ("scene", &shot.scene),
                ("description", &shot.description),
                ("dialogue", &shot.dialogue),
                ("camera", &shot.camera),
                ("notes", &shot.notes),
            ] {
                if let Some(value) = value {
                    validate_text(&format!("{field}.{name}"), value, MAX_TEXT_BYTES)?;
                    text_bytes += value.len();
                }
            }
            if let Some(frame) = &shot.frame {
                validate_image_path(&frame.path).map_err(|_| invalid(format!("{field}.frame.path"), "expected relative PNG/JPG/SVG path without dot components, backslash, colon or controls"))?;
                text_bytes += frame.path.len();
                if let Some(alt) = &frame.alt {
                    validate_text(&format!("{field}.frame.alt"), alt, MAX_TEXT_BYTES)?;
                    text_bytes += alt.len();
                }
            }
            if text_bytes > MAX_INPUT_BYTES {
                return Err(Error::Limit("16 MiB model text"));
            }
        }
        Ok(())
    }
}
/// Portable lexical asset validation. Symlink containment is checked at render time.
pub fn validate_image_path(path: &str) -> Result<(), Error> {
    let valid = !path.is_empty()
        && path.len() <= 4096
        && !Path::new(path).is_absolute()
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
        && Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ["png", "jpg", "svg"].contains(&e.to_ascii_lowercase().as_str()));
    if valid {
        Ok(())
    } else {
        Err(invalid("frame.path", "invalid local image path"))
    }
}
/// Non-drop duration labels. Hours may exceed 23; no wrap or wall-clock conversion.
pub fn format_timecode(frames: u64, rate: FrameRate) -> Result<String, Error> {
    let fps = rate.nominal()?;
    let seconds = frames / fps;
    Ok(format!(
        "{:02}:{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        frames % fps
    ))
}
/// A quoted Typst code string, safe even for markup, quotes, backslashes and controls.
/// Rendered user text is always passed to `text()` in code, never parsed as markup.
pub fn escape_typst(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn truncate(value: &str, limit: usize) -> String {
    let value: String = value
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    if value.chars().count() <= limit {
        value
    } else {
        value
            .chars()
            .take(limit.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    }
}
fn caption_line(value: &str, bold: bool, italic: bool, limit: usize, width: f64) -> String {
    // The box clips unusually wide glyph runs without affecting cell/page geometry.
    format!(
        "box(width: {width:.4}pt, height: 10pt, clip: true, text(weight: {}, style: {}, {}))",
        if bold { "\"bold\"" } else { "\"regular\"" },
        if italic { "\"italic\"" } else { "\"normal\"" },
        escape_typst(&truncate(value, limit))
    )
}

/// Render deterministic source. Validation errors are fatal; unavailable frames warn.
/// Source resolves images against Typst's `--root`, independent of the .typ subdirectory.
pub fn render_typst(board: &Board, options: &Options) -> Result<Rendered, Error> {
    board.validate()?;
    let layout = options.layout(board.aspect)?;
    let sheets = options.grid.page_count(board.shots.len());
    let pages = sheets + usize::from(options.title_page);
    let last = u64::from(options.start_page_number) + pages as u64 - 1;
    if last > u64::from(u32::MAX) {
        return Err(invalid("start_page_number", "page number overflow"));
    }
    let title = board.title.as_deref().unwrap_or("Storyboard");
    let header_limit = (layout.page_width / 7.0).floor() as usize;
    let mut source=format!("// SPDX-License-Identifier: MIT OR Apache-2.0\n// Generated deterministically by storyboard-sheets; image paths use --root.\n#set page(width: {:.4}pt, height: {:.4}pt, margin: (left: 14mm, right: 14mm, top: 18mm, bottom: 16mm), header: align(left, text(size: 10pt, weight: \"bold\", {})), footer: context align(center, text(size: 8pt, \"Page \" + str(counter(page).get().first() + {}) + \" of {}\")))\n#set text(font: \"DejaVu Sans\", size: 8pt)\n#set par(spacing: 0pt, leading: 0pt)\n",
        layout.page_width, layout.page_height, escape_typst(&truncate(title,header_limit)), options.start_page_number - 1, last);
    if options.title_page {
        source.push_str(&format!("#align(center + horizon)[\n#text(size: 22pt, weight: \"bold\", {})\n#v(12pt)\n#text({})\n]\n#pagebreak()\n",escape_typst(&truncate(title,160)),escape_typst(&format!("{} shots",board.shots.len()))));
    }
    let root = fs::canonicalize(&options.assets_root).ok();
    let mut warnings = Vec::new();
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    let (cols, rows) = options.grid.dimensions();
    let inner_width = layout.cell_width - 12.0;
    // Conservative default glyph budget; hard clipping guarantees fixed line height.
    let caption_limit = options
        .max_caption_chars
        .min((inner_width / 5.0).floor().max(1.0) as usize);
    for page in 0..sheets {
        if page > 0 {
            source.push_str("#pagebreak()\n");
        }
        source.push_str(&format!(
            "#grid(columns: ({}), rows: ({}), column-gutter: 8pt, row-gutter: 8pt,\n",
            vec![format!("{:.4}pt", layout.cell_width); cols].join(", "),
            vec![format!("{:.4}pt", layout.cell_height); rows].join(", ")
        ));
        for shot in board
            .shots
            .iter()
            .skip(page * options.grid.cells_per_page())
            .take(options.grid.cells_per_page())
        {
            let resolved = shot.frame.as_ref().and_then(|frame| {
                let path = options.assets_root.join(&frame.path);
                let canonical = fs::canonicalize(&path).ok();
                match (root.as_ref(), canonical) {
                    (Some(root), Some(canonical))
                        if canonical.starts_with(root) && canonical.is_file() =>
                    {
                        Some(frame)
                    }
                    (Some(root), Some(canonical)) if !canonical.starts_with(root) => {
                        warnings.push(Warning {
                            shot_id: shot.id.clone(),
                            kind: WarningKind::OutsideAssetsRoot,
                        });
                        None
                    }
                    _ => {
                        warnings.push(Warning {
                            shot_id: shot.id.clone(),
                            kind: WarningKind::MissingFile,
                        });
                        None
                    }
                }
            });
            if shot.frame.is_none() {
                warnings.push(Warning {
                    shot_id: shot.id.clone(),
                    kind: WarningKind::NoFrame,
                });
            }
            let image = if let Some(frame) = resolved {
                if seen.insert(&frame.path) {
                    assets.push(PathBuf::from(&frame.path));
                }
                let alt = frame.alt.as_deref().unwrap_or(&shot.id);
                format!(
                    "image({}, width: {:.4}pt, height: {:.4}pt, fit: \"contain\", alt: {})",
                    escape_typst(&format!("/{}", frame.path)),
                    layout.frame_width,
                    layout.frame_height,
                    escape_typst(alt)
                )
            } else {
                format!("box(width: {:.4}pt, height: {:.4}pt, stroke: (paint: luma(60%), thickness: 0.6pt, dash: \"dashed\"), align(center + horizon, text(size: 8pt, {})))",layout.frame_width,layout.frame_height,escape_typst(&format!("{}\nno frame",truncate(&shot.id,20))))
            };
            let mut captions = vec![caption_line(
                &shot.id,
                true,
                false,
                caption_limit,
                inner_width,
            )];
            for field in &options.caption_fields {
                let timecode = if *field == CaptionField::Timecode && options.show_timecodes {
                    match (shot.duration_frames, board.frame_rate) {
                        (Some(d), Some(r)) => Some(format_timecode(d, r)?),
                        _ => None,
                    }
                } else {
                    None
                };
                let value = match field {
                    CaptionField::Name => shot.name.as_deref(),
                    CaptionField::Scene => shot.scene.as_deref(),
                    CaptionField::Camera => shot.camera.as_deref(),
                    CaptionField::Timecode => timecode.as_deref(),
                    CaptionField::Description => shot.description.as_deref(),
                    CaptionField::Dialogue => shot.dialogue.as_deref(),
                    CaptionField::Notes => shot.notes.as_deref(),
                };
                if let Some(value) = value.filter(|v| !v.is_empty()) {
                    captions.push(caption_line(
                        value,
                        false,
                        *field == CaptionField::Dialogue,
                        caption_limit,
                        inner_width,
                    ));
                }
            }
            source.push_str(&format!("  block(width: {:.4}pt, height: {:.4}pt, inset: 6pt, clip: true, stroke: 0.4pt + luma(80%), stack(dir: ttb, spacing: 0pt, align(center, {}), v(4pt), stack(dir: ttb, spacing: 1pt, {}))),\n",layout.cell_width,layout.cell_height,image,captions.join(", ")));
        }
        if board.shots.is_empty() {
            source.push_str("  box[No shots],\n");
        }
        source.push_str(")\n");
    }
    Ok(Rendered {
        source,
        warnings,
        assets,
    })
}
/// Captured successful compiler diagnostics (empty for the bundled samples).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileReport {
    pub stderr: String,
}
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
/// Compile using an optional executable path, otherwise `typst` on PATH.
/// A unique .typ is created in assets_root and removed on all compiler outcomes.
/// Output paths are relative to the caller's current directory and may be overwritten.
pub fn compile_pdf(
    rendered: &Rendered,
    out_pdf: impl AsRef<Path>,
    assets_root: impl AsRef<Path>,
    typst_path: Option<&Path>,
) -> Result<CompileReport, Error> {
    let root = fs::canonicalize(assets_root).map_err(|e| io("resolve assets root", e))?;
    let output = out_pdf.as_ref();
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| io("resolve output", e))?
            .join(output)
    };
    let (source_path, mut file) = loop {
        let name = format!(
            ".storyboard-sheets-{}-{}.typ",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = root.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io("create Typst source", e)),
        }
    };
    let result = (|| {
        file.write_all(rendered.source.as_bytes())
            .map_err(|e| io("write Typst source", e))?;
        file.flush().map_err(|e| io("flush Typst source", e))?;
        // No shell interpolation, environment changes or network access.
        let result = Command::new(typst_path.unwrap_or_else(|| Path::new("typst")))
            .arg("compile")
            .arg("--root")
            .arg(&root)
            .arg("--format")
            .arg("pdf")
            .arg(&source_path)
            .arg(&output)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::TypstNotFound
                } else {
                    io("run Typst", e)
                }
            })?;
        let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
        if !result.status.success() {
            return Err(Error::Compile {
                status: result.status.code(),
                stderr,
            });
        }
        Ok(CompileReport { stderr })
    })();
    drop(file);
    let cleanup = fs::remove_file(source_path).map_err(|e| io("remove Typst source", e));
    match result {
        Ok(report) => {
            cleanup?;
            Ok(report)
        }
        Err(e) => {
            let _ = cleanup;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_truncation_keeps_scalar_boundaries() {
        assert_eq!(truncate("Привет", 4), "При…");
    }
    #[test]
    fn whitespace_is_one_line() {
        assert_eq!(truncate("a\nb\rc\td", 20), "a b c d");
    }
    #[test]
    fn ellipsis_counts_toward_limit() {
        assert_eq!(truncate("abc", 1), "…");
        assert_eq!(truncate("abc", 3), "abc");
    }
    #[test]
    fn escaped_controls_use_typst_syntax() {
        assert_eq!(escape_typst("\0"), "\"\\u{0}\"");
    }
}
