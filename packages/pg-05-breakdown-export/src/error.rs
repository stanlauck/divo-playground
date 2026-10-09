// SPDX-License-Identifier: MIT OR Apache-2.0
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

/// Stable code and structural path. No source text, keys, IDs or filenames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub code: &'static str,
    pub path: String,
}
impl Error {
    pub(crate) fn new(code: &'static str, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.code, self.path)
    }
}
impl std::error::Error for Error {}
