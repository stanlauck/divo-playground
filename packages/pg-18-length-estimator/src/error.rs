// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

/// Result alias used by the whole crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Typed, input-free error. Messages never echo screenplay text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input exceeds [`crate::MAX_INPUT_BYTES`].
    InputTooLarge { bytes: usize, max: usize },
    /// The input has more lines than [`crate::MAX_INPUT_LINES`].
    TooManyLines { lines: usize, max: usize },
    /// A layout profile field is outside its documented range, or the
    /// profile JSON is malformed.
    InvalidProfile(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { bytes, max } => {
                write!(f, "input is {bytes} bytes; the maximum is {max} bytes")
            }
            Self::TooManyLines { lines, max } => {
                write!(f, "input has {lines} lines; the maximum is {max} lines")
            }
            Self::InvalidProfile(reason) => write!(f, "invalid layout profile: {reason}"),
        }
    }
}

impl std::error::Error for Error {}
