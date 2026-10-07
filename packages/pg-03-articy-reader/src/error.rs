// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{fmt, io};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid { path: String, message: &'static str },
    Limit(&'static str),
}
impl Error {
    pub(crate) fn invalid(path: impl Into<String>, message: &'static str) -> Self {
        Self::Invalid {
            path: path.into(),
            message,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => f.write_str("articy export I/O failed"),
            Self::Json(_) => f.write_str("invalid JSON export"),
            Self::Invalid { path, message } => write!(f, "invalid export at {path}: {message}"),
            Self::Limit(name) => write!(f, "articy import limit exceeded: {name}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
