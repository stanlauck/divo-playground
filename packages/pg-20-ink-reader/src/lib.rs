// SPDX-License-Identifier: MIT OR Apache-2.0
//! Static, bounded reader of compiled Ink JSON. It never executes story code.
mod model;
mod reader;
pub use model::*;
pub use reader::read_compiled;
use serde::{Deserialize, Serialize};
use std::{fmt, io};

/// Input bounds and the independent output package's name.
#[derive(Clone, Debug)]
pub struct ReadOptions {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_elements: usize,
    pub package_name: String,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            max_bytes: 32 * 1024 * 1024,
            max_depth: 256,
            max_elements: 1_000_000,
            package_name: "stdin".into(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    Limit(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O: {e}"),
            Self::Json(e) => write!(f, "JSON: {e}"),
            Self::Invalid(e) => write!(f, "invalid compiled Ink: {e}"),
            Self::Limit(e) => write!(f, "limit exceeded: {e}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub version: u8,
    pub ink_version: u64,
    pub unsupported: Vec<UnsupportedFeature>,
    pub warnings: Vec<Finding>,
    pub stats: Stats,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnsupportedFeature {
    pub feature: String,
    pub count: usize,
    pub paths: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: String,
    pub path: String,
    pub message: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub knots: usize,
    pub stitches: usize,
    pub lines: usize,
    pub choices: usize,
    pub diverts: usize,
    pub variables: usize,
}
