// SPDX-License-Identifier: MIT OR Apache-2.0

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use pg_21_yarn::{read_files, DialogueGraph, ParseOptions, Severity};

const USAGE: &str = "\
yarn2graph - Yarn Spinner 2 .yarn to neutral dialogue-graph JSON

Usage: yarn2graph <in.yarn>... [-o out.json] [--strict]

  -o <path>   write the JSON document to <path> instead of stdout; `-o -` means stdout
  --strict    treat warnings as failures too
  -h, --help  show this text

Findings are printed to stderr as `file:line:col: severity[code]: message`.
The JSON document is written either way; the graph is best-effort so that a
report is still usable when the script has problems.

Exit codes: 0 no findings above the threshold, 1 findings above the threshold,
2 usage or input failure.";

/// Findings above the threshold make the process fail.
const EXIT_FINDINGS: u8 = 1;
/// Usage, read or write failures make the process fail.
const EXIT_FATAL: u8 = 2;

struct Args {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    strict: bool,
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args_os().skip(1)) {
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Some(args)) => args,
        Err(message) => {
            eprintln!("yarn2graph: {message}\n\n{USAGE}");
            return ExitCode::from(EXIT_FATAL);
        }
    };
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("yarn2graph: {message}");
            ExitCode::from(EXIT_FATAL)
        }
    }
}

fn run(args: &Args) -> Result<ExitCode, String> {
    let graph = read_files(&args.inputs, &ParseOptions::default()).map_err(|e| e.to_string())?;
    let json = pg_21_yarn::to_json(&graph).map_err(|e| e.to_string())?;
    report(&graph, args.strict);
    write_output(args, json.as_bytes())?;
    Ok(exit_code(&graph, args.strict))
}

fn exit_code(graph: &DialogueGraph, strict: bool) -> ExitCode {
    if failed(graph, strict) {
        ExitCode::from(EXIT_FINDINGS)
    } else {
        ExitCode::SUCCESS
    }
}

fn failed(graph: &DialogueGraph, strict: bool) -> bool {
    let failures = if strict {
        graph.finding_count()
    } else {
        graph.error_count()
    };
    failures > 0
}

fn report(graph: &DialogueGraph, strict: bool) {
    let mut stderr = std::io::stderr().lock();
    for finding in &graph.errors {
        let severity = match finding.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let code = finding.code.as_str();
        let _ = writeln!(
            stderr,
            "{}:{}:{}: {severity}[{code}]: {}",
            finding.file, finding.line, finding.col, finding.message
        );
    }
    if graph.errors.is_empty() {
        return;
    }
    let errors = graph.error_count();
    let warnings = graph.finding_count() - errors;
    let verdict = if failed(graph, strict) {
        "rejected"
    } else {
        "accepted"
    };
    let _ = writeln!(
        stderr,
        "yarn2graph: {errors} error(s), {warnings} warning(s) across {} file(s): {verdict}",
        graph.sources.len()
    );
}

fn write_output(args: &Args, json: &[u8]) -> Result<(), String> {
    let Some(path) = args
        .output
        .as_ref()
        .filter(|path| path.to_string_lossy() != "-")
    else {
        return std::io::stdout()
            .lock()
            .write_all(json)
            .map_err(|_| "cannot write to stdout".to_string());
    };
    let mut file =
        std::fs::File::create(path).map_err(|_| "cannot create the output file".to_string())?;
    file.write_all(json)
        .and_then(|()| file.flush())
        .map_err(|_| "cannot write the output file".to_string())
}

/// Returns `Ok(None)` for `--help`, `Ok(Some(args))` for a valid command line.
fn parse_args<I: Iterator<Item = T>, T: Into<std::ffi::OsString>>(
    arguments: I,
) -> Result<Option<Args>, String> {
    let mut args = Args {
        inputs: Vec::new(),
        output: None,
        strict: false,
    };
    let mut raw = arguments.map(|arg| PathBuf::from(arg.into()));
    while let Some(arg) = raw.next() {
        let text = arg.to_string_lossy();
        match text.as_ref() {
            "-h" | "--help" => return Ok(None),
            "--strict" => args.strict = true,
            "-o" => {
                let value = raw
                    .next()
                    .ok_or_else(|| "-o needs an output path".to_string())?;
                args.output = Some(value);
            }
            _ if text.starts_with("-o") && text.len() > 2 => {
                args.output = Some(PathBuf::from(text[2..].to_string()));
            }
            _ if text == "-" => {
                return Err("stdin input is not supported; name a .yarn file".to_string())
            }
            _ if text.starts_with('-') && text.len() > 1 => {
                return Err(format!("unknown option `{text}`"))
            }
            _ => args.inputs.push(arg),
        }
    }
    if args.inputs.is_empty() {
        return Err("no input file was given".to_string());
    }
    Ok(Some(args))
}
