// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, io::Read, path::Path};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SECTIONS: usize = 10_000;
pub const MAX_BLOCKS: usize = 100_000;
pub const MAX_DEPTH: usize = 8;

/// Errors from parsing, validation, file I/O, or the optional compiler.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    Limit(&'static str),
    Invalid(String),
    TypstMissing(String),
    TypstFailed { status: Option<i32>, stderr: String },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Json(e) => write!(f, "JSON: {e}"),
            Self::Limit(s) => write!(f, "limit exceeded: {s}"),
            Self::Invalid(s) => write!(f, "invalid report: {s}"),
            Self::TypstMissing(s) => write!(f, "Typst executable not found: {s}"),
            Self::TypstFailed { status, stderr } => {
                write!(f, "Typst failed ({status:?}): {stderr}")
            }
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Neutral report v1. Strings are plain text; metadata is never generated.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub sections: Vec<Section>,
}
fn yes() -> bool {
    true
}
fn one() -> u8 {
    1
}

/// Section levels are absolute, including in nested sections.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: String,
    pub title: String,
    #[serde(default = "one")]
    pub level: u8,
    #[serde(default)]
    pub optional: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalloutKind {
    Note,
    Warning,
    Error,
    Success,
}
impl CalloutKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Success => "Success",
        }
    }
}

/// All block strings are literal text. Image paths use portable relative syntax.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Paragraph {
        text: String,
    },
    Heading {
        text: String,
        level: u8,
    },
    Bullets {
        items: Vec<String>,
    },
    Numbered {
        items: Vec<String>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        #[serde(default)]
        caption: Option<String>,
    },
    KeyValues {
        pairs: Vec<(String, String)>,
    },
    Code {
        text: String,
        #[serde(default)]
        language: Option<String>,
    },
    Quote {
        text: String,
        #[serde(default)]
        attribution: Option<String>,
    },
    Callout {
        kind: CalloutKind,
        text: String,
    },
    PageBreak {},
    Image {
        path: String,
        #[serde(default)]
        caption: Option<String>,
        #[serde(default)]
        width_percent: Option<f64>,
    },
}

/// Parse at most 16 MiB and validate the full tree before rendering.
pub fn from_json(text: &str) -> Result<Report, Error> {
    if text.len() > MAX_INPUT_BYTES {
        return Err(Error::Limit("16 MiB input"));
    }
    let report: Report = serde_json::from_str(text)?;
    validate(&report)?;
    Ok(report)
}

/// Bounded reader: consumes at most the limit plus one byte, even for streams.
pub fn read_report(reader: impl Read) -> Result<Report, Error> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(Error::Limit("16 MiB input"));
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("input is not UTF-8".into()))?;
    from_json(text)
}

/// Validate programmatically constructed reports too. Duplicate IDs are errors.
pub fn validate(report: &Report) -> Result<(), Error> {
    if report.version != 1 {
        return Err(Error::Invalid("version must be 1".into()));
    }
    if report.title.trim().is_empty() {
        return Err(Error::Invalid("title must not be empty".into()));
    }
    if let Some(date) = &report.date {
        if !valid_date(date) {
            return Err(Error::Invalid(
                "date must be an ISO YYYY-MM-DD calendar date".into(),
            ));
        }
    }
    if let Some(lang) = &report.language {
        if !valid_language(lang) {
            return Err(Error::Invalid(
                "language must be a well-formed BCP-47 tag".into(),
            ));
        }
    }
    let mut stack: Vec<_> = report.sections.iter().rev().map(|s| (s, 1)).collect();
    let mut ids = HashSet::new();
    let mut blocks = 0usize;
    while let Some((s, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(Error::Limit("section depth 8"));
        }
        if s.id.trim().is_empty() || !ids.insert(&s.id) {
            return Err(Error::Invalid(format!(
                "empty or duplicate section id {:?}",
                s.id
            )));
        }
        if ids.len() > MAX_SECTIONS {
            return Err(Error::Limit("10,000 sections"));
        }
        if s.title.trim().is_empty() {
            return Err(Error::Invalid(format!(
                "empty title for section {:?}",
                s.id
            )));
        }
        valid_level(s.level)?;
        blocks = blocks
            .checked_add(s.blocks.len())
            .ok_or(Error::Limit("100,000 blocks"))?;
        if blocks > MAX_BLOCKS {
            return Err(Error::Limit("100,000 blocks"));
        }
        for b in &s.blocks {
            match b {
                Block::Heading { level, .. } => valid_level(*level)?,
                Block::Table { columns, rows, .. } => {
                    if columns.is_empty() || rows.iter().any(|r| r.len() != columns.len()) {
                        return Err(Error::Invalid(
                            "table needs columns and rectangular rows".into(),
                        ));
                    }
                }
                Block::Code {
                    language: Some(lang),
                    ..
                } => {
                    if lang.is_empty()
                        || !lang
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_+-".contains(&b))
                    {
                        return Err(Error::Invalid(
                            "code language must be a nonempty alphanumeric token (also _ + -)"
                                .into(),
                        ));
                    }
                }
                Block::Image {
                    path,
                    width_percent,
                    ..
                } => {
                    valid_image_path(path)?;
                    if width_percent.is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 100.0) {
                        return Err(Error::Invalid(
                            "image width_percent must be > 0 and <= 100".into(),
                        ));
                    }
                }
                _ => {}
            }
        }
        stack.extend(s.sections.iter().rev().map(|child| (child, depth + 1)));
    }
    Ok(())
}
fn valid_level(level: u8) -> Result<(), Error> {
    if !(1..=4).contains(&level) {
        return Err(Error::Invalid("heading level must be 1..4".into()));
    }
    Ok(())
}

pub(crate) fn valid_image_path(path: &str) -> Result<(), Error> {
    // Cross-platform policy, even when running on Unix. No URL/scheme, query,
    // fragment, controls, backslashes, empty or traversal components.
    if path.is_empty()
        || Path::new(path).is_absolute()
        || path.chars().any(|c| c.is_control() || "\\:?#".contains(c))
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(Error::Invalid(format!("unsafe image path {path:?}")));
    }
    Ok(())
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year = s[..4].parse::<u32>().unwrap_or(0);
    let month = s[5..7].parse::<usize>().unwrap_or(0);
    let day = s[8..].parse::<u32>().unwrap_or(0);
    let days = [
        31,
        if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year > 0 && (1..=12).contains(&month) && day > 0 && day <= days[month - 1]
}

fn valid_language(s: &str) -> bool {
    // RFC 5646 syntax (no registry lookup); grandfathered tags are accepted.
    const OLD: &[&str] = &[
        "en-gb-oed",
        "i-ami",
        "i-bnn",
        "i-default",
        "i-enochian",
        "i-hak",
        "i-klingon",
        "i-lux",
        "i-mingo",
        "i-navajo",
        "i-pwn",
        "i-tao",
        "i-tay",
        "i-tsu",
        "sgn-be-fr",
        "sgn-be-nl",
        "sgn-ch-de",
        "art-lojban",
        "cel-gaulish",
        "no-bok",
        "no-nyn",
        "zh-guoyu",
        "zh-hakka",
        "zh-min",
        "zh-min-nan",
        "zh-xiang",
    ];
    let lower = s.to_ascii_lowercase();
    if OLD.contains(&lower.as_str()) {
        return true;
    }
    let parts: Vec<_> = lower.split('-').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return false;
    }
    if parts[0] == "x" {
        return parts.len() > 1;
    }
    let alpha = |p: &str| p.bytes().all(|b| b.is_ascii_alphabetic());
    if !(2..=8).contains(&parts[0].len()) || !alpha(parts[0]) {
        return false;
    }
    let mut i = 1;
    if parts[0].len() <= 3 {
        for _ in 0..3 {
            if i < parts.len() && parts[i].len() == 3 && alpha(parts[i]) {
                i += 1;
            } else {
                break;
            }
        }
    }
    if i < parts.len() && parts[i].len() == 4 && alpha(parts[i]) {
        i += 1;
    }
    if i < parts.len()
        && ((parts[i].len() == 2 && alpha(parts[i]))
            || (parts[i].len() == 3 && parts[i].bytes().all(|b| b.is_ascii_digit())))
    {
        i += 1;
    }
    let mut variants = HashSet::new();
    while i < parts.len()
        && ((5..=8).contains(&parts[i].len())
            || (parts[i].len() == 4 && parts[i].as_bytes()[0].is_ascii_digit()))
    {
        if !variants.insert(parts[i]) {
            return false;
        }
        i += 1;
    }
    let mut singletons = HashSet::new();
    while i < parts.len() && parts[i].len() == 1 && parts[i] != "x" {
        if !singletons.insert(parts[i]) {
            return false;
        }
        i += 1;
        let start = i;
        while i < parts.len() && parts[i].len() >= 2 {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if i < parts.len() && parts[i] == "x" {
        return i + 1 < parts.len();
    }
    i == parts.len()
}
