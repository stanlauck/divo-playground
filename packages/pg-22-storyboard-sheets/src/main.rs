// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};
use storyboard_sheets::{compile_pdf, read_board, render_typst, Grid, Options, Page};

const USAGE: &str = "Usage: storyboard-sheets <shots.json> (--typ out.typ | --pdf out.pdf) [--assets DIR] [--grid 2x3|3x4] [--page a4|letter] [--landscape] [--timecodes] [--typst PATH]";
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or(USAGE)?;
    if input == "--help" || input == "-h" {
        println!("{USAGE}");
        return Ok(());
    }
    if input.to_string_lossy().starts_with('-') {
        return Err(USAGE.into());
    }
    let mut options = Options {
        assets_root: Path::new(&input)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        ..Options::default()
    };
    let mut typ: Option<PathBuf> = None;
    let mut pdf: Option<PathBuf> = None;
    let mut typst: Option<PathBuf> = None;
    let mut seen = std::collections::HashSet::new();
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("option must be UTF-8")?;
        if !seen.insert(flag.to_owned()) {
            return Err(format!("duplicate option: {flag}").into());
        }
        match flag {
            "--typ" => typ = Some(args.next().ok_or("--typ needs a path")?.into()),
            "--pdf" => pdf = Some(args.next().ok_or("--pdf needs a path")?.into()),
            "--assets" => {
                options.assets_root = args.next().ok_or("--assets needs a directory")?.into()
            }
            "--typst" => typst = Some(args.next().ok_or("--typst needs a path")?.into()),
            "--grid" => {
                options.grid = match args.next().as_deref().and_then(|s| s.to_str()) {
                    Some("2x3") => Grid::TwoByThree,
                    Some("3x4") => Grid::ThreeByFour,
                    _ => return Err("--grid must be 2x3 or 3x4".into()),
                }
            }
            "--page" => {
                options.page = match args.next().as_deref().and_then(|s| s.to_str()) {
                    Some("a4") => Page::A4,
                    Some("letter") => Page::Letter,
                    _ => return Err("--page must be a4 or letter".into()),
                }
            }
            "--landscape" => options.landscape = true,
            "--timecodes" => options.show_timecodes = true,
            _ => return Err(format!("unknown option: {flag}\n{USAGE}").into()),
        }
    }
    if typ.is_some() == pdf.is_some() {
        return Err("select exactly one of --typ or --pdf".into());
    }
    if typ.is_some() && typst.is_some() {
        return Err("--typst requires --pdf".into());
    }
    let board = read_board(std::fs::File::open(&input)?)?;
    let rendered = render_typst(&board, &options)?;
    for warning in &rendered.warnings {
        eprintln!("warning: shot {:?}: {:?}", warning.shot_id, warning.kind);
    }
    if let Some(path) = typ {
        std::fs::write(path, &rendered.source)?;
    }
    if let Some(path) = pdf {
        let report = compile_pdf(&rendered, path, &options.assets_root, typst.as_deref())?;
        if !report.stderr.is_empty() {
            eprint!("{}", report.stderr);
        }
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("storyboard-sheets: {e}");
        std::process::exit(1);
    }
}
