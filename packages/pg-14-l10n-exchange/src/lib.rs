// SPDX-License-Identifier: MIT OR Apache-2.0

//! Localization exchange for string tables.
//!
//! A [`StringTable`] is a flat list of translatable lines, each with optional
//! per-line context, character name, translator note, length limit and
//! review state. It converts losslessly between three formats:
//!
//! - JSON ([`from_json`], [`to_json`]), the canonical form;
//! - XLIFF 2.1 ([`from_xliff`], [`to_xliff`]) for CAT tools;
//! - CSV ([`from_csv`], [`to_csv`]) for spreadsheets.
//!
//! Empty optional strings are treated as absent in every format.

mod csv_format;
mod error;
mod model;
mod xliff;

pub use csv_format::{CSV_COLUMNS, from_csv, to_csv};
pub use error::{Error, Result};
pub use model::{Entry, LengthViolation, SCHEMA_VERSION, State, StringTable};
pub use xliff::{SLR_NS, XLIFF_NS, from_xliff, to_xliff};

/// Parses and validates a string table from JSON.
pub fn from_json(json: &str) -> Result<StringTable> {
    let table: StringTable = serde_json::from_str(json)?;
    table.validate()?;
    Ok(table)
}

/// Validates and serializes a string table as pretty-printed JSON.
pub fn to_json(table: &StringTable) -> Result<String> {
    table.validate()?;
    let mut out = serde_json::to_string_pretty(table)?;
    out.push('\n');
    Ok(out)
}
