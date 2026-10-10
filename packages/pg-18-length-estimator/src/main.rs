// SPDX-License-Identifier: MIT OR Apache-2.0

use screenplay_length::{
    estimate_with, to_json, to_table, LayoutProfile, PageSize, MAX_INPUT_BYTES,
};
use std::ffi::OsString;
use std::io::{self, Read, Write};

const USAGE: &str =
    "Usage: screenplay-length <file.fountain | -> [--a4] [--json] [--profile profile.json]\n\
  --a4       use the A4 layout profile (default: US Letter)\n\
  --json     print the JSON estimate instead of the table\n\
  --profile  load layout numbers from a JSON file (overrides --a4)\n\
  -          read the screenplay from standard input";

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        if args.is_empty() {
            std::process::exit(2);
        }
        return;
    }
    match run(args) {
        Ok(text) => {
            let mut stdout = io::stdout().lock();
            if stdout.write_all(text.as_bytes()).is_err() {
                std::process::exit(1);
            }
        }
        Err(message) => {
            eprintln!("screenplay-length: {message}");
            std::process::exit(1);
        }
    }
}

fn run(args: Vec<OsString>) -> Result<String, String> {
    let mut input: Option<OsString> = None;
    let mut a4 = false;
    let mut json = false;
    let mut profile_path: Option<OsString> = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--a4") => a4 = true,
            Some("--json") => json = true,
            Some("--profile") => {
                profile_path = Some(iter.next().ok_or("--profile needs a file path")?);
            }
            Some(s) if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(arg).is_some() {
                    return Err("exactly one input file is expected".to_string());
                }
            }
        }
    }
    let input = input.ok_or("missing input file")?;
    let profile = match profile_path {
        Some(path) => {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("cannot read profile: {e}"))?;
            LayoutProfile::from_json(&text).map_err(|e| e.to_string())?
        }
        None if a4 => PageSize::A4.profile(),
        None => PageSize::Letter.profile(),
    };
    let text = read_input(&input)?;
    let estimate = estimate_with(&text, &profile).map_err(|e| e.to_string())?;
    Ok(if json {
        to_json(&estimate)
    } else {
        to_table(&estimate)
    })
}

fn read_input(path: &OsString) -> Result<String, String> {
    let mut bytes = Vec::new();
    if path == "-" {
        io::stdin()
            .lock()
            .take(MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
    } else {
        let file = std::fs::File::open(path).map_err(|e| format!("cannot open input: {e}"))?;
        file.take(MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read input: {e}"))?;
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(format!("input exceeds {MAX_INPUT_BYTES} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| "input is not valid UTF-8".to_string())
}
