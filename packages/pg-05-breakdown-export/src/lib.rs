// SPDX-License-Identifier: MIT OR Apache-2.0
//! Export data-only production breakdowns, without scheduling or pagination.
#![doc = include_str!("../README.md")]

mod category;
mod csv_format;
mod error;
mod fdx;
mod json;
mod model;

pub use category::Category;
pub use csv_format::{to_csv, to_csv_with_mode, CsvMode, CSV_COLUMNS};
pub use error::{Error, Result};
pub use fdx::to_fdx;
pub use json::from_json;
pub use model::{
    Breakdown, Element, ElementRef, IntExt, Scene, ShootingDay, TimeOfDay, MAX_INPUT_BYTES,
    MAX_OUTPUT_BYTES, MAX_PAGE_EIGHTHS, MAX_RECORDS, MAX_REFERENCES, MAX_TEXT_BYTES,
};

/// Validate and serialize the neutral representation, not an FDX import/roundtrip.
pub fn to_json(value: &Breakdown) -> Result<String> {
    value.validate()?;
    let mut text = serde_json::to_string_pretty(value).map_err(|_| Error::new("json", "$"))?;
    text.push('\n');
    if text.len() > MAX_INPUT_BYTES {
        return Err(Error::new("input_limit", "$"));
    }
    Ok(text)
}
