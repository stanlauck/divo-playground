// SPDX-License-Identifier: MIT OR Apache-2.0

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use subtitle_writer::{
    check, parse, read_input, report_json, write, Checks, Format, MAX_INPUT_BYTES,
};

const USAGE: &str = "Usage: subtitle-writer <in.json> [--format srt|vtt|ttml] [-o OUT] [--report REPORT.json] [--strict]
                       [--max-cps N] [--max-line-length N] [--max-lines N]
                       [--min-duration-ms N] [--max-duration-ms N]

  <in.json>           neutral dialogue timing JSON v1 (`-` reads stdin)
  --format FORMAT     write subtitles in FORMAT to stdout (or to -o OUT);
                      without --format the violations report is written to stdout
  -o, --output OUT    subtitle output file (created or overwritten)
  --report REPORT     write the violations report (JSON array) to this file;
                      with --format and no --report, a non-empty report goes to stderr
  --strict            exit with status 1 when there is any violation
  --max-cps N         reading speed limit, characters per second (default 17)
  --max-line-length N characters per line (default 42)
  --max-lines N       lines per cue (default 2)
  --min-duration-ms N minimum cue duration (default 1000)
  --max-duration-ms N maximum cue duration (default 7000)

Exit status: 0 success, 1 violations under --strict, 2 usage or input error.";

struct Options {
    input: PathBuf,
    format: Option<Format>,
    output: Option<PathBuf>,
    report: Option<PathBuf>,
    strict: bool,
    checks: Checks,
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: Vec<OsString>) -> Result<Options, String> {
    let mut input = None;
    let mut format = None;
    let mut output = None;
    let mut report = None;
    let mut strict = false;
    let mut checks = Checks::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let text = arg.to_str().map(str::to_owned);
        let mut value = |name: &str| -> Result<OsString, String> {
            iter.next().ok_or_else(|| format!("{name} needs a value"))
        };
        match text.as_deref() {
            Some("--format") => {
                let v = value("--format")?;
                let name = v.to_str().ok_or("--format must be srt, vtt or ttml")?;
                format = Some(Format::parse(name).ok_or("--format must be srt, vtt or ttml")?);
            }
            Some("-o") | Some("--output") => output = Some(PathBuf::from(value("-o")?)),
            Some("--report") => report = Some(PathBuf::from(value("--report")?)),
            Some("--strict") => strict = true,
            Some("--max-cps") => checks.max_cps = number(&value("--max-cps")?, "--max-cps")?,
            Some("--max-line-length") => {
                checks.max_line_length = number(&value("--max-line-length")?, "--max-line-length")?
            }
            Some("--max-lines") => {
                checks.max_lines = number(&value("--max-lines")?, "--max-lines")?
            }
            Some("--min-duration-ms") => {
                checks.min_duration_ms = number(&value("--min-duration-ms")?, "--min-duration-ms")?
            }
            Some("--max-duration-ms") => {
                checks.max_duration_ms = number(&value("--max-duration-ms")?, "--max-duration-ms")?
            }
            Some(other) if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option {other}"));
            }
            _ => {
                if input.replace(PathBuf::from(arg)).is_some() {
                    return Err("only one input file is accepted".into());
                }
            }
        }
    }
    if output.is_some() && format.is_none() {
        return Err("-o requires --format".into());
    }
    checks.validate().map_err(|e| e.to_string())?;
    Ok(Options {
        input: input.ok_or("missing input file")?,
        format,
        output,
        report,
        strict,
        checks,
    })
}

fn number<T: std::str::FromStr>(value: &OsString, name: &str) -> Result<T, String> {
    value
        .to_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("{name} must be a non-negative integer"))
}

/// Returns `Ok(true)` on success, `Ok(false)` when `--strict` found violations.
fn run(options: &Options) -> Result<bool, String> {
    let text = if options.input.as_os_str() == "-" {
        read_input(io::stdin().lock(), MAX_INPUT_BYTES).map_err(|e| e.to_string())?
    } else {
        let file =
            File::open(&options.input).map_err(|e| format!("cannot open input: {}", e.kind()))?;
        read_input(file, MAX_INPUT_BYTES).map_err(|e| e.to_string())?
    };
    let document = parse(&text).map_err(|e| e.to_string())?;
    let violations = check(&document, &options.checks).map_err(|e| e.to_string())?;
    let report = report_json(&violations);

    if let Some(format) = options.format {
        let subtitles = write(&document, format).map_err(|e| e.to_string())?;
        match &options.output {
            Some(path) => std::fs::write(path, subtitles)
                .map_err(|e| format!("cannot write output: {}", e.kind()))?,
            None => emit(io::stdout().lock(), &subtitles)?,
        }
        match &options.report {
            Some(path) => std::fs::write(path, report)
                .map_err(|e| format!("cannot write report: {}", e.kind()))?,
            None if !violations.is_empty() => emit(io::stderr().lock(), &report)?,
            None => {}
        }
    } else {
        match &options.report {
            Some(path) => std::fs::write(path, report)
                .map_err(|e| format!("cannot write report: {}", e.kind()))?,
            None => emit(io::stdout().lock(), &report)?,
        }
    }
    Ok(!(options.strict && !violations.is_empty()))
}

fn emit(mut sink: impl Write, text: &str) -> Result<(), String> {
    sink.write_all(text.as_bytes())
        .and_then(|_| sink.flush())
        .map_err(|e| format!("cannot write: {}", e.kind()))
}
