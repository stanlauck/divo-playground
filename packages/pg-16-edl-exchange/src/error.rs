// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

/// Safe diagnosis without input contents, filenames or private parser causes.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub(crate) fn invalid(path: impl Into<String>) -> Self {
        Self::new("invalid_value", path)
    }
    pub(crate) fn unsupported(path: impl Into<String>) -> Self {
        Self::new("unsupported", path)
    }
    pub(crate) fn at(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.code, self.path)
    }
}
impl std::error::Error for Error {}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new("invalid_json", "input")
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        let code = match error.kind() {
            std::io::ErrorKind::AlreadyExists => "already_exists",
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::BrokenPipe => "broken_pipe",
            _ => "io_error",
        };
        Self::new(code, "input/output")
    }
}
