// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Cue, Document, Error, Result};
use serde::Serialize;
use std::collections::HashMap;

/// Default `max_cps` (characters per second).
pub const DEFAULT_MAX_CPS: u32 = 17;
/// Default `max_line_length` (Unicode scalar values per line).
pub const DEFAULT_MAX_LINE_LENGTH: usize = 42;
/// Default `max_lines` (lines per cue).
pub const DEFAULT_MAX_LINES: usize = 2;
/// Default `min_duration_ms`.
pub const DEFAULT_MIN_DURATION_MS: u64 = 1000;
/// Default `max_duration_ms`.
pub const DEFAULT_MAX_DURATION_MS: u64 = 7000;

/// Readability limits. All limits are inclusive: a value *equal* to a limit
/// is acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checks {
    /// Maximum reading speed, characters per second (`> 0`).
    pub max_cps: u32,
    /// Maximum characters per line (`> 0`).
    pub max_line_length: usize,
    /// Maximum lines per cue (`> 0`).
    pub max_lines: usize,
    /// Minimum cue duration in milliseconds (`> 0`).
    pub min_duration_ms: u64,
    /// Maximum cue duration in milliseconds (`>= min_duration_ms`).
    pub max_duration_ms: u64,
}

impl Default for Checks {
    fn default() -> Self {
        Self {
            max_cps: DEFAULT_MAX_CPS,
            max_line_length: DEFAULT_MAX_LINE_LENGTH,
            max_lines: DEFAULT_MAX_LINES,
            min_duration_ms: DEFAULT_MIN_DURATION_MS,
            max_duration_ms: DEFAULT_MAX_DURATION_MS,
        }
    }
}

impl Checks {
    /// Rejects zero limits and `min_duration_ms > max_duration_ms`.
    pub fn validate(&self) -> Result<()> {
        if self.max_cps == 0 {
            return Err(Error::Config("max_cps must be greater than zero"));
        }
        if self.max_line_length == 0 {
            return Err(Error::Config("max_line_length must be greater than zero"));
        }
        if self.max_lines == 0 {
            return Err(Error::Config("max_lines must be greater than zero"));
        }
        if self.min_duration_ms == 0 {
            return Err(Error::Config("min_duration_ms must be greater than zero"));
        }
        if self.max_duration_ms < self.min_duration_ms {
            return Err(Error::Config(
                "max_duration_ms must not be smaller than min_duration_ms",
            ));
        }
        Ok(())
    }
}

/// Identifier of a check. Serialized in `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// The cue id was already used by an earlier cue.
    DuplicateId,
    /// A line is empty or whitespace-only.
    EmptyLine,
    /// `end_ms <= start_ms`.
    EndNotAfterStart,
    /// The cue starts before the previous cue in the array.
    NotSorted,
    /// The cue starts before the previous cue ends.
    Overlap,
    /// Duration below `min_duration_ms`.
    TooShort,
    /// Duration above `max_duration_ms`.
    TooLong,
    /// More lines than `max_lines`.
    TooManyLines,
    /// A line has more characters than `max_line_length`.
    LineTooLong,
    /// Characters per second above `max_cps`.
    ReadingSpeed,
}

/// A measured number: integers stay integers in JSON, reading speed is a
/// float rounded to two decimals.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Metric {
    Int(i64),
    Float(f64),
}

/// One readability or structural problem.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Violation {
    /// Id of the cue the violation belongs to.
    pub cue: String,
    /// Which rule was broken.
    pub rule: Rule,
    /// Deterministic human-readable explanation. Never quotes input text.
    pub message: String,
    /// The measured value.
    pub value: Metric,
    /// The limit it was compared against.
    pub limit: Metric,
}

/// Runs every check with the given limits.
///
/// Violations are ordered by cue (input order), then by rule in the order of
/// the [`Rule`] enum, then by line index. Only [`Error::Config`] is possible.
pub fn check(document: &Document, checks: &Checks) -> Result<Vec<Violation>> {
    checks.validate()?;
    let mut out = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut previous: Option<&Cue> = None;
    for cue in &document.cues {
        let count = seen.entry(cue.id.as_str()).or_insert(0);
        *count += 1;
        let mut push = |rule: Rule, message: String, value: Metric, limit: Metric| {
            out.push(Violation {
                cue: cue.id.clone(),
                rule,
                message,
                value,
                limit,
            });
        };
        if *count > 1 {
            push(
                Rule::DuplicateId,
                format!("id used by {count} cues"),
                Metric::Int(*count as i64),
                Metric::Int(1),
            );
        }
        for (i, line) in cue.lines.iter().enumerate() {
            if line.trim().is_empty() {
                push(
                    Rule::EmptyLine,
                    format!("line {i} is empty"),
                    Metric::Int(0),
                    Metric::Int(1),
                );
            }
        }
        let duration = cue.duration_ms();
        if duration <= 0 {
            push(
                Rule::EndNotAfterStart,
                format!("duration is {duration} ms, end must be after start"),
                Metric::Int(duration),
                Metric::Int(1),
            );
        }
        if let Some(prev) = previous {
            if cue.start_ms < prev.start_ms {
                push(
                    Rule::NotSorted,
                    format!(
                        "starts at {} ms, before the previous cue at {} ms",
                        cue.start_ms, prev.start_ms
                    ),
                    Metric::Int(cue.start_ms as i64),
                    Metric::Int(prev.start_ms as i64),
                );
            } else if cue.start_ms < prev.end_ms {
                let overlap = prev.end_ms - cue.start_ms;
                push(
                    Rule::Overlap,
                    format!("overlaps the previous cue by {overlap} ms"),
                    Metric::Int(overlap as i64),
                    Metric::Int(0),
                );
            }
        }
        if duration > 0 {
            let duration_u = duration as u64;
            if duration_u < checks.min_duration_ms {
                push(
                    Rule::TooShort,
                    format!(
                        "duration {duration_u} ms is below the minimum of {} ms",
                        checks.min_duration_ms
                    ),
                    Metric::Int(duration),
                    Metric::Int(checks.min_duration_ms as i64),
                );
            }
            if duration_u > checks.max_duration_ms {
                push(
                    Rule::TooLong,
                    format!(
                        "duration {duration_u} ms is above the maximum of {} ms",
                        checks.max_duration_ms
                    ),
                    Metric::Int(duration),
                    Metric::Int(checks.max_duration_ms as i64),
                );
            }
        }
        if cue.lines.len() > checks.max_lines {
            push(
                Rule::TooManyLines,
                format!("{} lines, maximum is {}", cue.lines.len(), checks.max_lines),
                Metric::Int(cue.lines.len() as i64),
                Metric::Int(checks.max_lines as i64),
            );
        }
        for (i, line) in cue.lines.iter().enumerate() {
            let chars = line.chars().count();
            if chars > checks.max_line_length {
                push(
                    Rule::LineTooLong,
                    format!(
                        "line {i} has {chars} characters, maximum is {}",
                        checks.max_line_length
                    ),
                    Metric::Int(chars as i64),
                    Metric::Int(checks.max_line_length as i64),
                );
            }
        }
        if duration > 0 {
            let chars = cue.char_count() as u64;
            let duration_u = duration as u64;
            // Exact integer comparison: chars / (duration / 1000) > max_cps.
            if chars * 1000 > u64::from(checks.max_cps) * duration_u {
                let cps = reading_speed(chars, duration_u);
                push(
                    Rule::ReadingSpeed,
                    format!(
                        "{chars} characters in {duration_u} ms is {cps} characters per second, maximum is {}",
                        checks.max_cps
                    ),
                    Metric::Float(cps),
                    Metric::Int(i64::from(checks.max_cps)),
                );
            }
        }
        previous = Some(cue);
    }
    Ok(out)
}

/// Characters per second rounded to two decimals (`duration_ms > 0`).
pub fn reading_speed(chars: u64, duration_ms: u64) -> f64 {
    // Round half up in integer arithmetic so the result is platform-independent.
    let hundredths = (chars * 100_000 + duration_ms / 2) / duration_ms.max(1);
    hundredths as f64 / 100.0
}

/// Serializes violations as pretty JSON (2-space indent, LF, trailing newline).
pub fn report_json(violations: &[Violation]) -> String {
    let mut text = serde_json::to_string_pretty(violations).expect("violations always serialize");
    text.push('\n');
    text
}
