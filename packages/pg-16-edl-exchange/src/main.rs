// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{
    Error, FrameRate, MAX_EDL_BYTES, MAX_JSON_BYTES, Result, from_edl, from_json, read_input,
    to_edl, to_json,
};
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};

const USAGE: &str = "Usage: edl-exchange --from json|edl --to json|edl --input PATH|- --output PATH|- [--fps NUM[/DEN]]\nExternal EDL requires --fps; FCM determines drop/non-drop mode. Output files must not exist.";

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
    if !matches!(args.len(), 8 | 10) {
        return Err(argument());
    }
    let (mut from, mut to, mut input, mut output, mut fps) = (None, None, None, None, None);
    for pair in args.chunks_exact(2) {
        let slot = match pair[0].to_str() {
            Some("--from") => &mut from,
            Some("--to") => &mut to,
            Some("--input") => &mut input,
            Some("--output") => &mut output,
            Some("--fps") => &mut fps,
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
    let fps = fps.map(|s| rate(&s)).transpose()?;
    if from == "json" && fps.is_some() {
        return Err(argument());
    }
    let limit = if from == "json" {
        MAX_JSON_BYTES
    } else {
        MAX_EDL_BYTES
    };
    let text = if input == "-" {
        read_input(io::stdin().lock(), limit)?
    } else {
        read_input(File::open(input).map_err(|e| io_error(e, "input"))?, limit)?
    };
    let list = if from == "json" {
        from_json(&text)?
    } else {
        from_edl(&text, fps)?
    };
    // Validate and serialize before creating the output; never overwrite.
    let text = if to == "json" {
        to_json(&list)?
    } else {
        to_edl(&list)?
    };
    if output == "-" {
        let mut out = io::stdout().lock();
        out.write_all(text.as_bytes())
            .map_err(|e| io_error(e, "output"))?;
        out.flush().map_err(|e| io_error(e, "output"))?;
    } else {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|e| io_error(e, "output"))?;
        out.write_all(text.as_bytes())
            .map_err(|e| io_error(e, "output"))?;
        out.flush().map_err(|e| io_error(e, "output"))?;
    }
    Ok(())
}
fn format(value: Option<&OsStr>) -> Result<&str> {
    match value.and_then(OsStr::to_str) {
        Some(name @ ("json" | "edl")) => Ok(name),
        _ => Err(argument()),
    }
}
fn rate(value: &OsStr) -> Result<FrameRate> {
    let s = value.to_str().ok_or_else(argument)?;
    let (n, d) = s.split_once('/').unwrap_or((s, "1"));
    if n.is_empty() || d.is_empty() || !n.bytes().chain(d.bytes()).all(|b| b.is_ascii_digit()) {
        return Err(argument());
    }
    let r = FrameRate::new(
        n.parse().map_err(|_| argument())?,
        d.parse().map_err(|_| argument())?,
    );
    r.validate()?;
    Ok(r)
}
fn io_error(cause: io::Error, side: &'static str) -> Error {
    // The binary is a separate crate, so `Error::at` (pub(crate)) is out of reach.
    let mut error = Error::from(cause);
    error.path = side.into();
    error
}
fn argument() -> Error {
    Error {
        code: "invalid_arguments",
        path: "arguments (use --help)".into(),
    }
}
