// SPDX-License-Identifier: MIT OR Apache-2.0

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use timeline_writer::{
    Error, MAX_JSON_BYTES, MAX_TIMELINE_BYTES, Result, ShotList, from_fcpxml, from_json, from_otio,
    read_input, to_fcpxml, to_json, to_otio,
};

const USAGE: &str = "Usage: timeline-writer --from json|otio|fcpxml --to json|otio|fcpxml --input PATH|- --output PATH|-";

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.as_slice() == [OsString::from("--help")] {
        println!("{USAGE}");
        return;
    }
    if let Err(error) = run(args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<OsString>) -> Result<()> {
    let mut from = None;
    let mut to = None;
    let mut input = None;
    let mut output = None;
    if args.len() != 8 {
        return Err(argument());
    }
    for pair in args.chunks_exact(2) {
        let slot = match pair[0].to_str() {
            Some("--from") => &mut from,
            Some("--to") => &mut to,
            Some("--input") => &mut input,
            Some("--output") => &mut output,
            _ => return Err(argument()),
        };
        if slot.replace(pair[1].clone()).is_some() {
            return Err(argument());
        }
    }
    let from = format(from.as_deref())?;
    let to = format(to.as_deref())?;
    let input = input.ok_or_else(argument)?;
    let output = output.ok_or_else(argument)?;
    let limit = if from == "json" {
        MAX_JSON_BYTES
    } else {
        MAX_TIMELINE_BYTES
    };
    let text = if input == "-" {
        read_input(io::stdin().lock(), limit)?
    } else {
        read_input(File::open(input)?, limit)?
    };
    let list: ShotList = match from {
        "json" => from_json(&text)?,
        "otio" => from_otio(&text)?,
        _ => from_fcpxml(&text)?,
    };
    // Validate and serialize completely before creating the output.
    let text = match to {
        "json" => to_json(&list)?,
        "otio" => to_otio(&list)?,
        _ => to_fcpxml(&list)?,
    };
    if output == "-" {
        let mut out = io::stdout().lock();
        out.write_all(text.as_bytes())?;
        out.flush()?;
    } else {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        out.write_all(text.as_bytes())?;
        out.flush()?;
    }
    Ok(())
}

fn format(value: Option<&std::ffi::OsStr>) -> Result<&str> {
    match value.and_then(|v| v.to_str()) {
        Some(name @ ("json" | "otio" | "fcpxml")) => Ok(name),
        _ => Err(argument()),
    }
}
fn argument() -> Error {
    Error {
        code: "invalid_arguments",
        path: "arguments (use --help)".into(),
    }
}
