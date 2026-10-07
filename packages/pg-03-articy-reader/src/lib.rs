// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read a native, single-file articy:draft JSON export without executing scripts.

#![doc = include_str!("../README.md")]

mod error;
mod json;
mod model;
mod reader;

pub use error::{Error, Result};
pub use model::*;
pub use reader::read_export;
