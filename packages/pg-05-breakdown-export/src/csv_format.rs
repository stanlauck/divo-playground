// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{Breakdown, Error, Result, MAX_OUTPUT_BYTES};
use std::{
    collections::HashMap,
    io::{self, Write},
};

pub const CSV_COLUMNS: &[&str] = &[
    "order",
    "shooting_day_id",
    "shooting_day",
    "date",
    "scene_id",
    "scene_number",
    "int_ext",
    "set",
    "time_of_day",
    "pages_eighths",
    "pages",
    "synopsis",
    "script_day",
    "unit",
    "elements_json",
    "notes",
];
/// Quoting alone does not prevent spreadsheet formula interpretation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CsvMode {
    #[default]
    SpreadsheetSafe,
    /// Faithful cells for programmatic consumers, never a safe spreadsheet mode.
    Raw,
}
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_OUTPUT_BYTES {
            return Err(io::Error::other("output limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn to_csv(value: &Breakdown) -> Result<String> {
    to_csv_with_mode(value, CsvMode::SpreadsheetSafe)
}
pub fn to_csv_with_mode(value: &Breakdown, mode: CsvMode) -> Result<String> {
    value.validate()?;
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .quote_style(csv::QuoteStyle::Always)
        .from_writer(Limited(Vec::new()));
    writer.write_record(CSV_COLUMNS).map_err(csv_error)?;
    let elements: HashMap<_, _> = value.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    for (i, (scene, day)) in value.ordered_scenes().into_iter().enumerate() {
        let element_values: Vec<_> = scene
            .elements
            .iter()
            .map(|item| {
                let element = elements[item.element_id.as_str()];
                serde_json::json!({"id":element.id,"category":element.category,
                "name":element.name,"quantity":item.quantity})
            })
            .collect();
        let elements_json =
            serde_json::to_string(&element_values).map_err(|_| Error::new("json", "$.csv"))?;
        let mut row = vec![
            (i + 1).to_string(),
            day.map(|d| d.id.as_str()).unwrap_or("").into(),
            day.map(|d| d.label.as_str()).unwrap_or("").into(),
            day.and_then(|d| d.date.as_deref()).unwrap_or("").into(),
            scene.id.clone(),
            scene.number.clone(),
            scene.int_ext.slug().into(),
            scene.set.clone(),
            scene.time_of_day.slug().into(),
            scene.pages_eighths.to_string(),
            pages(scene.pages_eighths),
            scene.synopsis.clone(),
            scene.script_day.clone().unwrap_or_default(),
            scene.unit.clone().unwrap_or_default(),
            elements_json,
            scene.notes.clone(),
        ];
        if mode == CsvMode::SpreadsheetSafe {
            for value in &mut row {
                let start = value.trim_start_matches(|c: char| c.is_whitespace() || c.is_control());
                if start.starts_with(['=', '+', '-', '@']) {
                    value.insert(0, '\'');
                }
            }
        }
        writer.write_record(row).map_err(csv_error)?;
    }
    writer
        .flush()
        .map_err(|_| Error::new("output_limit", "$.csv"))?;
    let bytes = writer
        .into_inner()
        .map_err(|_| Error::new("output_limit", "$.csv"))?
        .0;
    String::from_utf8(bytes).map_err(|_| Error::new("encoding", "$.csv"))
}
fn pages(eighths: u32) -> String {
    let whole = eighths / 8;
    let rest = eighths % 8;
    match (whole, rest) {
        (_, 0) => whole.to_string(),
        (0, _) => format!("{rest}/8"),
        _ => format!("{whole} {rest}/8"),
    }
}
fn csv_error(_: csv::Error) -> Error {
    Error::new("output_limit", "$.csv")
}
