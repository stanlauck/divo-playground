// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// The input is not JSON or does not have the shape of the schema.
    Json(serde_json::Error),
    /// The input parsed but breaks a rule of the schema.
    Invalid { path: String, message: String },
}

impl Error {
    pub(crate) fn invalid(path: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Invalid {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Json(e) => write!(f, "invalid shot JSON: {e}"),
            Error::Invalid { path, message } => write!(f, "{path}: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Json(e) => Some(e),
            Error::Invalid { .. } => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
