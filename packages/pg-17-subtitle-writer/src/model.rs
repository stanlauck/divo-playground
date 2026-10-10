// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;

/// The only supported value of the top-level `version` field.
pub const SCHEMA_VERSION: u64 = 1;
/// Byte budget for the JSON input (parse and `read_input`).
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
/// Parsing budget for `cues.length`.
pub const MAX_CUES: usize = 100_000;
/// Parsing budget for `cues[].lines.length` (distinct from the `max_lines` check).
pub const MAX_LINES_PER_CUE: usize = 16;
/// Parsing budget for one line, in Unicode scalar values (distinct from the
/// `max_line_length` check).
pub const MAX_LINE_CHARS: usize = 1024;
/// Parsing budget for `id` and `speaker`, in Unicode scalar values.
pub const MAX_NAME_CHARS: usize = 256;
/// Parsing budget for `title`, in Unicode scalar values.
pub const MAX_TITLE_CHARS: usize = 1024;
/// Largest accepted time: `99:59:59.999`, so every timecode has two-digit hours.
pub const MAX_TIME_MS: u64 = 359_999_999;
/// Parsing budget for JSON nesting depth (arrays/objects), including in
/// ignored unknown fields.
pub const MAX_DEPTH: usize = 32;

/// A neutral dialogue timing document (JSON v1).
///
/// Cues are kept in input order; the writer never sorts them. Ordering and
/// overlap problems are reported by [`check`](crate::check).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Must be `1`.
    pub version: u64,
    /// Optional title; only the TTML output carries it (`ttm:title`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional BCP-47 language tag; only the TTML output carries it (`xml:lang`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Cues in presentation order.
    pub cues: Vec<Cue>,
}

/// One subtitle cue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cue {
    /// Non-empty identifier. Uniqueness is a check, not a parse error.
    pub id: String,
    /// Optional speaker label. `null` and absence are equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// One or more lines as displayed; no line may contain a line break.
    pub lines: Vec<String>,
    /// Start time, integer wall-clock milliseconds.
    pub start_ms: u64,
    /// End time, integer wall-clock milliseconds (exclusive).
    pub end_ms: u64,
}

impl Document {
    /// An empty version-1 document.
    pub fn new() -> Self {
        Self {
            version: SCHEMA_VERSION,
            title: None,
            language: None,
            cues: Vec::new(),
        }
    }

    /// Structural validation: the same rules `parse` applies, for documents
    /// built in Rust. Writers call this before emitting anything.
    pub fn validate(&self) -> Result<()> {
        if self.version != SCHEMA_VERSION {
            return Err(Error::invalid("version", "must be 1"));
        }
        if let Some(title) = &self.title {
            text("title", title, MAX_TITLE_CHARS)?;
        }
        if let Some(language) = &self.language {
            if language.is_empty()
                || language.len() > 64
                || !language
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err(Error::invalid(
                    "language",
                    "must be a non-empty ASCII tag of letters, digits and hyphens",
                ));
            }
        }
        if self.cues.len() > MAX_CUES {
            return Err(Error::limit("cues", self.cues.len(), MAX_CUES));
        }
        for (index, cue) in self.cues.iter().enumerate() {
            cue.validate(index)?;
        }
        Ok(())
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Cue {
    /// A cue without a speaker.
    pub fn new(id: impl Into<String>, start_ms: u64, end_ms: u64, lines: &[&str]) -> Self {
        Self {
            id: id.into(),
            speaker: None,
            lines: lines.iter().map(|s| (*s).to_owned()).collect(),
            start_ms,
            end_ms,
        }
    }

    /// Signed duration in milliseconds (negative when `end_ms < start_ms`).
    pub fn duration_ms(&self) -> i64 {
        self.end_ms as i64 - self.start_ms as i64
    }

    /// Characters counted for reading speed: Unicode scalar values of all
    /// lines, without line breaks and without the speaker label.
    pub fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.chars().count()).sum()
    }

    fn validate(&self, index: usize) -> Result<()> {
        let path = format!("cues[{index}]");
        text(format!("{path}.id"), &self.id, MAX_NAME_CHARS)?;
        if self.id.is_empty() {
            return Err(Error::invalid(format!("{path}.id"), "must not be empty"));
        }
        if let Some(speaker) = &self.speaker {
            text(format!("{path}.speaker"), speaker, MAX_NAME_CHARS)?;
            if speaker.trim().is_empty() {
                return Err(Error::invalid(
                    format!("{path}.speaker"),
                    "must not be empty (omit it or use null)",
                ));
            }
        }
        if self.lines.is_empty() {
            return Err(Error::invalid(
                format!("{path}.lines"),
                "must contain at least one line",
            ));
        }
        if self.lines.len() > MAX_LINES_PER_CUE {
            return Err(Error::limit(
                format!("{path}.lines"),
                self.lines.len(),
                MAX_LINES_PER_CUE,
            ));
        }
        for (i, line) in self.lines.iter().enumerate() {
            text(format!("{path}.lines[{i}]"), line, MAX_LINE_CHARS)?;
        }
        if self.start_ms > MAX_TIME_MS {
            return Err(Error::limit(
                format!("{path}.start_ms"),
                clamp(self.start_ms),
                MAX_TIME_MS as usize,
            ));
        }
        if self.end_ms > MAX_TIME_MS {
            return Err(Error::limit(
                format!("{path}.end_ms"),
                clamp(self.end_ms),
                MAX_TIME_MS as usize,
            ));
        }
        Ok(())
    }
}

fn clamp(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Text fields: bounded length, no C0/C1 controls (so no line breaks or tabs),
/// no Unicode line/paragraph separators, no `-->` (a WebVTT/SRT timing marker).
fn text(path: impl Into<String>, value: &str, limit: usize) -> Result<()> {
    let path = path.into();
    let count = value.chars().count();
    if count > limit {
        return Err(Error::limit(path, count, limit));
    }
    if value
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{fffe}' | '\u{ffff}'))
    {
        return Err(Error::invalid(
            path,
            "must not contain control characters or line/paragraph separators",
        ));
    }
    if value.contains("-->") {
        return Err(Error::invalid(path, "must not contain \"-->\""));
    }
    Ok(())
}

/// Parses and validates neutral JSON v1 from text.
///
/// The input must not exceed [`MAX_INPUT_BYTES`]. A leading UTF-8 BOM is
/// skipped. Unknown fields are ignored. Times must be non-negative integers.
pub fn parse(text: &str) -> Result<Document> {
    if text.len() > MAX_INPUT_BYTES {
        return Err(Error::InputTooLarge {
            bytes: text.len(),
            limit: MAX_INPUT_BYTES,
        });
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let depth = nesting_depth(text);
    if depth > MAX_DEPTH {
        return Err(Error::limit("input", depth, MAX_DEPTH));
    }
    let document: Document = serde_json::from_str(text)?;
    document.validate()?;
    Ok(document)
}

/// Maximum bracket nesting of the JSON text (strings are skipped). A linear
/// scan, so malformed text yields some number; the real parse decides validity.
fn nesting_depth(text: &str) -> usize {
    let (mut depth, mut max, mut in_string, mut escaped) = (0usize, 0usize, false, false);
    for b in text.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                max = max.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// Reads at most `limit` bytes of UTF-8 (`limit` is capped at [`MAX_INPUT_BYTES`]).
pub fn read_input(reader: impl Read, limit: usize) -> Result<String> {
    let limit = limit.min(MAX_INPUT_BYTES);
    let mut bytes = Vec::new();
    reader.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::InputTooLarge {
            bytes: bytes.len(),
            limit,
        });
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_depth_scan() {
        assert_eq!(nesting_depth(""), 0);
        assert_eq!(nesting_depth("{}"), 1);
        assert_eq!(nesting_depth(r#"{"a":[[1],{"b":[]}]}"#), 4);
        assert_eq!(nesting_depth(r#"{"a":"[[[[{{{{"}"#), 1);
        assert_eq!(nesting_depth(r#"{"a":"x\"[[[["}"#), 1);
        assert_eq!(nesting_depth("]]]]{"), 1);
    }

    #[test]
    fn cue_helpers() {
        let cue = Cue::new("a", 500, 200, &["ab", "cd"]);
        assert_eq!(cue.duration_ms(), -300);
        assert_eq!(cue.char_count(), 4);
        assert_eq!(Cue::new("a", 0, 1, &["ё", "ab"]).char_count(), 3);
    }
}
