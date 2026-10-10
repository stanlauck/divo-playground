// SPDX-License-Identifier: MIT OR Apache-2.0

//! Failure modes that abort a run entirely, as opposed to per-line findings
//! that are returned separately in [`crate::ParseReport`].

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// A source file could not be read. The OS error text is not embedded, so
    /// reports never carry host filesystem details.
    Io { source: String },
    /// A source file is not valid UTF-8.
    NotUtf8 { source: String },
    /// A source file exceeds the configured byte bound.
    TooLarge {
        source: String,
        bytes: u64,
        limit: u64,
    },
    /// Parsing produced more statement nodes than the configured bound.
    TooManyNodes { source: String, limit: usize },
    /// Parsing produced more findings than the configured bound.
    TooManyFindings { source: String, limit: usize },
    /// A [`crate::ParseOptions`] limit was zero.
    InvalidLimits,
    /// No source was supplied.
    NoInput,
    /// The graph could not be serialized.
    Encode,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { source } => write!(f, "{source}: cannot read input"),
            Self::NotUtf8 { source } => write!(f, "{source}: input is not valid UTF-8"),
            Self::TooLarge {
                source,
                bytes,
                limit,
            } => {
                write!(
                    f,
                    "{source}: input is {bytes} bytes, over the {limit} byte limit"
                )
            }
            Self::TooManyNodes { source, limit } => {
                write!(f, "{source}: produced more than {limit} nodes")
            }
            Self::TooManyFindings { source, limit } => {
                write!(f, "{source}: produced more than {limit} findings")
            }
            Self::InvalidLimits => write!(f, "every parse limit must be greater than zero"),
            Self::NoInput => write!(f, "no input file was given"),
            Self::Encode => write!(f, "the graph could not be serialized"),
        }
    }
}

impl std::error::Error for Error {}
