// SPDX-License-Identifier: MIT OR Apache-2.0
use pg_05_breakdown_export::{from_json, to_csv_with_mode, to_fdx, CsvMode, Error};
use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, Write},
    process::ExitCode,
};

const USAGE: &str = "Usage: pg-05-breakdown-export <fdx|csv> <input.json|-> <output|-> [--raw-csv]\nOutput files must not already exist. Use --raw-csv only for programmatic consumers.";
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    if !(3..=4).contains(&args.len())
        || (args[0] != "fdx" && args[0] != "csv")
        || (args.len() == 4 && (args[3] != "--raw-csv" || args[0] != "csv"))
    {
        return Err(USAGE.into());
    }
    let input = if args[1] == "-" {
        from_json(io::stdin().lock())
    } else {
        from_json(File::open(&args[1]).map_err(|_| "read at $".to_string())?)
    }
    .map_err(|e| e.to_string())?;
    let output = if args[0] == "fdx" {
        to_fdx(&input)
    } else {
        to_csv_with_mode(
            &input,
            if args.len() == 4 {
                CsvMode::Raw
            } else {
                CsvMode::SpreadsheetSafe
            },
        )
    }
    .map_err(|e: Error| e.to_string())?;
    if args[2] == "-" {
        io::stdout().lock().write_all(output.as_bytes())
    } else {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])
            .map_err(|_| "create at $.output".to_string())?;
        file.write_all(output.as_bytes())
    }
    .map_err(|_| "write at $.output".to_string())
}
