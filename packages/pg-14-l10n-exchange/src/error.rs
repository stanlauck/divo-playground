// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Json(serde_json::Error),
    Xml(roxmltree::Error),
    Csv(csv::Error),
    Io(std::io::Error),
    /// The input is well-formed but breaks a rule of the string table model.
    Invalid(String),
    /// The input uses a feature of the exchange format that this crate does not map.
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Json(e) => write!(f, "JSON: {e}"),
            Error::Xml(e) => write!(f, "XML: {e}"),
            Error::Csv(e) => write!(f, "CSV: {e}"),
            Error::Io(e) => write!(f, "I/O: {e}"),
            Error::Invalid(msg) => write!(f, "invalid string table: {msg}"),
            Error::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Json(e) => Some(e),
            Error::Xml(e) => Some(e),
            Error::Csv(e) => Some(e),
            Error::Io(e) => Some(e),
            Error::Invalid(_) | Error::Unsupported(_) => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<roxmltree::Error> for Error {
    fn from(e: roxmltree::Error) -> Self {
        Error::Xml(e)
    }
}

impl From<csv::Error> for Error {
    fn from(e: csv::Error) -> Self {
        Error::Csv(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
