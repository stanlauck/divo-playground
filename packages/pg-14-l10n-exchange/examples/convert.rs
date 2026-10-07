// SPDX-License-Identifier: MIT OR Apache-2.0

//! Converts a string table between JSON, XLIFF 2.1 and CSV.
//!
//! ```text
//! cargo run --example convert -- <input> <output> [source_lang] [target_lang]
//! ```
//!
//! Formats are picked by extension (`.json`, `.xlf`/`.xliff`, `.csv`).
//! Languages are only needed when the input is CSV.

use std::path::Path;
use std::process::ExitCode;

use l10n_exchange::{StringTable, from_csv, from_json, from_xliff, to_csv, to_json, to_xliff};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, rest @ ..] = args.as_slice() else {
        return Err("usage: convert <input> <output> [source_lang] [target_lang]".into());
    };

    let data = std::fs::read_to_string(input)?;
    let table: StringTable = match ext(input).as_str() {
        "json" => from_json(&data)?,
        "xlf" | "xliff" => from_xliff(&data)?,
        "csv" => {
            let src = rest
                .first()
                .ok_or("CSV input needs a source_lang argument")?;
            from_csv(data.as_bytes(), src, rest.get(1).map(String::as_str))?
        }
        other => return Err(format!("unknown input extension {other:?}").into()),
    };

    for v in table.length_violations() {
        eprintln!(
            "warning: {} is {} code points, limit {}",
            v.id, v.actual, v.max_length
        );
    }

    let out = match ext(output).as_str() {
        "json" => to_json(&table)?,
        "xlf" | "xliff" => to_xliff(&table)?,
        "csv" => to_csv(&table)?,
        other => return Err(format!("unknown output extension {other:?}").into()),
    };
    std::fs::write(output, out)?;
    Ok(())
}

fn ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}
