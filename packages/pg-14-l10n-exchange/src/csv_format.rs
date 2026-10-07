// SPDX-License-Identifier: MIT OR Apache-2.0

//! CSV mapping: one row per entry, a fixed header, empty cell = absent.
//! Languages are not part of the file and are passed by the caller.

use std::io::Read;

use crate::error::{Error, Result};
use crate::model::{Entry, SCHEMA_VERSION, State, StringTable, none_if_empty};

/// Column order written by [`to_csv`]. [`from_csv`] accepts any order and
/// requires only `id` and `source`.
pub const CSV_COLUMNS: [&str; 8] = [
    "id",
    "source",
    "target",
    "context",
    "character",
    "note",
    "max_length",
    "state",
];

/// Writes the table as RFC 4180 CSV with a header row and CRLF line endings.
pub fn to_csv(table: &StringTable) -> Result<String> {
    table.validate()?;
    let mut w = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_writer(Vec::new());
    w.write_record(CSV_COLUMNS)?;
    for e in &table.strings {
        let max = e.max_length.map(|m| m.to_string()).unwrap_or_default();
        let state = e.state.map(State::as_str).unwrap_or_default();
        w.write_record([
            e.id.as_str(),
            e.source.as_str(),
            e.target.as_deref().unwrap_or_default(),
            e.context.as_deref().unwrap_or_default(),
            e.character.as_deref().unwrap_or_default(),
            e.note.as_deref().unwrap_or_default(),
            max.as_str(),
            state,
        ])?;
    }
    let bytes = w.into_inner().map_err(|e| Error::Io(e.into_error()))?;
    String::from_utf8(bytes).map_err(|e| Error::Invalid(e.to_string()))
}

/// Reads CSV produced by [`to_csv`] or by a spreadsheet. A UTF-8 byte order
/// mark is skipped. Unknown columns are rejected so typos do not silently
/// drop data.
pub fn from_csv(
    input: impl Read,
    source_lang: &str,
    target_lang: Option<&str>,
) -> Result<StringTable> {
    // Flexible because spreadsheets often drop trailing empty cells;
    // over-long rows are still rejected below.
    let mut r = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(input);

    let headers = r.headers()?.clone();
    let mut index = [None::<usize>; CSV_COLUMNS.len()];
    for (pos, raw) in headers.iter().enumerate() {
        let name = raw.trim_start_matches('\u{feff}').trim();
        let col = CSV_COLUMNS
            .iter()
            .position(|c| *c == name)
            .ok_or_else(|| Error::Invalid(format!("unknown CSV column {name:?}")))?;
        if index[col].replace(pos).is_some() {
            return Err(Error::Invalid(format!("duplicate CSV column {name:?}")));
        }
    }
    for required in [0, 1] {
        if index[required].is_none() {
            return Err(Error::Invalid(format!(
                "missing CSV column {:?}",
                CSV_COLUMNS[required]
            )));
        }
    }

    let mut table = StringTable {
        version: SCHEMA_VERSION,
        source_lang: source_lang.to_owned(),
        target_lang: none_if_empty(target_lang.map(str::to_owned)),
        strings: Vec::new(),
    };
    for (row, record) in r.records().enumerate() {
        let record = record?;
        let line = record.position().map_or(row as u64 + 2, |p| p.line());
        if record.len() > headers.len() {
            return Err(Error::Invalid(format!(
                "line {line}: {} cells but {} columns",
                record.len(),
                headers.len()
            )));
        }
        let cell = |col: usize| -> Option<String> {
            none_if_empty(index[col].and_then(|i| record.get(i)).map(str::to_owned))
        };
        let max_length = cell(6)
            .map(|v| {
                v.trim().parse::<u32>().map_err(|_| {
                    Error::Invalid(format!("line {line}: max_length {v:?} is not a number"))
                })
            })
            .transpose()?;
        let state = cell(7)
            .map(|v| v.trim().parse::<State>())
            .transpose()
            .map_err(|e| Error::Invalid(format!("line {line}: {e}")))?;
        table.strings.push(Entry {
            id: cell(0).unwrap_or_default(),
            source: cell(1).unwrap_or_default(),
            target: cell(2),
            context: cell(3),
            character: cell(4),
            note: cell(5),
            max_length,
            state,
        });
    }
    table.validate()?;
    Ok(table)
}
