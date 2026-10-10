// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

/// Hard failures: malformed or out-of-budget input, or an invalid check
/// configuration. Readability problems are *not* errors; they are reported as
/// [`Violation`](crate::Violation)s.
///
/// Messages name structural locations (`cues[3].lines[1]`) and counts, never
/// the input text itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input exceeds the byte budget.
    InputTooLarge { bytes: usize, limit: usize },
    /// The input is not valid UTF-8.
    InvalidUtf8,
    /// The input is not well-formed JSON or does not match the v1 shape.
    InvalidJson { line: usize, column: usize },
    /// A value at `path` is present but unacceptable.
    Invalid { path: String, reason: &'static str },
    /// A count or size at `path` exceeds a parsing budget.
    Limit {
        path: String,
        value: usize,
        limit: usize,
    },
    /// The check configuration is inconsistent.
    Config(&'static str),
    /// Reading the input or writing the output failed.
    Io(std::io::ErrorKind),
}

impl Error {
    pub(crate) fn invalid(path: impl Into<String>, reason: &'static str) -> Self {
        Self::Invalid {
            path: path.into(),
            reason,
        }
    }
    pub(crate) fn limit(path: impl Into<String>, value: usize, limit: usize) -> Self {
        Self::Limit {
            path: path.into(),
            value,
            limit,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { bytes, limit } => {
                write!(f, "input too large: {bytes} bytes, limit {limit}")
            }
            Self::InvalidUtf8 => f.write_str("input is not valid UTF-8"),
            Self::InvalidJson { line, column } => {
                write!(f, "invalid JSON at line {line} column {column}")
            }
            Self::Invalid { path, reason } => write!(f, "invalid value at {path}: {reason}"),
            Self::Limit { path, value, limit } => {
                write!(f, "limit exceeded at {path}: {value} > {limit}")
            }
            Self::Config(reason) => write!(f, "invalid check configuration: {reason}"),
            Self::Io(kind) => write!(f, "i/o error: {kind}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        // Only the position is kept; serde messages may quote input text.
        Self::InvalidJson {
            line: error.line(),
            column: error.column(),
        }
    }
}
