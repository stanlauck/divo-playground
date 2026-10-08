// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod classify;
mod error;
mod extract;
mod model;
mod preflight;

pub use error::{Error, Result};
pub use model::*;
use std::io::Read;

/// Import a text-layer PDF. No OCR, filesystem discovery, network or JavaScript execution.
pub fn import_pdf<R: Read>(reader: R, options: &ImportOptions) -> Result<Screenplay> {
    options.validate()?;
    let mut screenplay = extract::extract(reader, options)?;
    classify::classify(&mut screenplay, options)?;
    Ok(screenplay)
}
