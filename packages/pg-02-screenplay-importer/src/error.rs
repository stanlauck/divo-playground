// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidPdf,
    PasswordProtected,
    InvalidOptions,
    InvalidGeometry,
    Limit(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => f.write_str("PDF input read failed"),
            Self::InvalidPdf => f.write_str("invalid or unsupported PDF"),
            Self::PasswordProtected => {
                f.write_str("PDF requires a password or unsupported decryption")
            }
            Self::InvalidOptions => {
                f.write_str("limits must be positive and Form depth at most 32")
            }
            Self::InvalidGeometry => f.write_str("invalid or nonfinite PDF geometry"),
            Self::Limit(name) => write!(f, "import limit exceeded: {name}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
