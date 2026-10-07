// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    InvalidConfiguration(&'static str),
    InvalidInput(&'static str),
    Transport(reqwest::Error),
    Http { status: u16 },
    Decode(serde_json::Error),
    Protocol(&'static str),
    ResponseTooLarge { limit: usize },
    WaitTimeout,
    ExecutionFailed,
    ExecutionInterrupted,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(reason) => {
                write!(formatter, "invalid configuration: {reason}")
            }
            Self::InvalidInput(reason) => write!(formatter, "invalid input: {reason}"),
            Self::Transport(_) => formatter.write_str("ComfyUI transport request failed"),
            Self::Http { status } => write!(formatter, "ComfyUI returned HTTP {status}"),
            Self::Decode(_) => formatter.write_str("ComfyUI returned invalid JSON"),
            Self::Protocol(reason) => write!(formatter, "invalid ComfyUI response: {reason}"),
            Self::ResponseTooLarge { limit } => {
                write!(formatter, "ComfyUI response exceeds the {limit}-byte limit")
            }
            Self::WaitTimeout => formatter.write_str("waiting for ComfyUI exceeded its deadline"),
            Self::ExecutionFailed => formatter.write_str("ComfyUI job failed"),
            Self::ExecutionInterrupted => formatter.write_str("ComfyUI job was interrupted"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.without_url())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error)
    }
}
