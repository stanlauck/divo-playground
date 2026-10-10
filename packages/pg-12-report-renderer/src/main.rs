// SPDX-License-Identifier: MIT OR Apache-2.0
use report_renderer::{
    compile_pdf, read_report, render_markdown, render_typst, Block, PageSize, RenderOptions, Report,
};
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const USAGE: &str = "report-renderer <in.json> (--md out.md | --typ out.typ | --pdf out.pdf) [--typst /path] [--include a,b] [--exclude c] [--include-optional] [--toc] [--page a4|letter]";

struct Args {
    input: PathBuf,
    output: PathBuf,
    format: String,
    typst: Option<PathBuf>,
    options: RenderOptions,
}
fn value(args: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<OsString, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("missing value for {flag}"))?;
    if value.to_string_lossy().starts_with("--") {
        return Err(format!("missing value for {flag}"));
    }
    Ok(value)
}
fn ids(value: OsString) -> Result<Vec<String>, String> {
    let value = value
        .into_string()
        .map_err(|_| "section IDs must be UTF-8")?;
    let ids: Vec<_> = value.split(',').map(|id| id.trim().to_owned()).collect();
    if ids.iter().any(String::is_empty) {
        return Err("section ID lists cannot contain empty IDs".into());
    }
    Ok(ids)
}
fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Args, String> {
    let input = args.next().ok_or(USAGE)?;
    if input.to_string_lossy().starts_with('-') {
        return Err("expected input JSON path first".into());
    }
    let mut output = None;
    let mut format = String::new();
    let mut typst = None;
    let mut options = RenderOptions::default();
    while let Some(flag) = args.next() {
        let flag = flag.into_string().map_err(|_| "option must be UTF-8")?;
        match flag.as_str() {
            "--md" | "--typ" | "--pdf" => {
                if output.is_some() {
                    return Err("choose exactly one output format".into());
                }
                output = Some(PathBuf::from(value(&mut args, &flag)?));
                format = flag;
            }
            "--typst" => {
                if typst.is_some() {
                    return Err("--typst may only be specified once".into());
                }
                typst = Some(PathBuf::from(value(&mut args, &flag)?));
            }
            "--include" => options
                .include
                .get_or_insert_with(Vec::new)
                .extend(ids(value(&mut args, &flag)?)?),
            "--exclude" => options.exclude.extend(ids(value(&mut args, &flag)?)?),
            "--include-optional" => options.include_optional = true,
            "--toc" => options.toc = true,
            "--page" => {
                options.page = match value(&mut args, &flag)?.to_str() {
                    Some("a4") => PageSize::A4,
                    Some("letter") => PageSize::Letter,
                    _ => return Err("--page must be a4 or letter".into()),
                };
            }
            _ => return Err(format!("unknown option {flag:?}")),
        }
    }
    if typst.is_some() && format != "--pdf" {
        return Err("--typst requires --pdf".into());
    }
    Ok(Args {
        input: input.into(),
        output: output.ok_or("output format is required")?,
        format,
        typst,
        options,
    })
}
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
struct Staging(PathBuf);
impl Staging {
    fn create(directory: &Path) -> Result<Self, std::io::Error> {
        for i in 0..1000 {
            let path = directory.join(format!(".report-renderer-{}-{i}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "cannot create PDF staging directory",
        ))
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Prefix assets in a cloned report to prevent collisions with report.typ/pdf.
// Only accessible files under the JSON's directory are copied. Typst still
// diagnoses missing images itself through the renderer's placeholder warnings.
fn stage_images(
    report: &mut Report,
    options: &RenderOptions,
    source_root: &Path,
    dest: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_root = source_root.canonicalize()?;
    let includes = options.include.as_deref().unwrap_or(&[]);
    let mut stack: Vec<_> = report.sections.iter_mut().rev().collect();
    while let Some(s) = stack.pop() {
        if !s.enabled
            || options.exclude.contains(&s.id)
            || (s.optional && !options.include_optional && !includes.contains(&s.id))
        {
            continue;
        }
        for b in &mut s.blocks {
            if let Block::Image { path, .. } = b {
                if let Ok(source) = source_root.join(&*path).canonicalize() {
                    if source.starts_with(&source_root) && source.is_file() {
                        let target = dest.join("assets").join(&*path);
                        fs::create_dir_all(parent(&target))?;
                        fs::copy(source, target)?;
                    }
                }
                *path = format!("assets/{path}");
            }
        }
        stack.extend(s.sections.iter_mut().rev());
    }
    Ok(())
}
fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let input = args.input.canonicalize()?;
    if args.output.canonicalize().is_ok_and(|p| p == input) {
        return Err("output must not overwrite input JSON".into());
    }
    let mut report = read_report(fs::File::open(&input)?)?;
    let mut options = args.options;
    if args.format == "--pdf" {
        let staging = Staging::create(parent(&args.output))?;
        stage_images(&mut report, &options, parent(&input), &staging.0)?;
        options.image_root = Some(staging.0.clone());
        let result = render_typst(&report, &options);
        for warning in result.warnings {
            eprintln!("warning: {warning}");
        }
        let pdf = compile_pdf(&result.text, &staging.0, args.typst.as_deref())?;
        fs::copy(pdf, &args.output)?;
    } else {
        // Relative references in .typ are interpreted next to that document.
        options.image_root = Some(parent(&args.output).to_owned());
        let result = if args.format == "--md" {
            render_markdown(&report, &options)
        } else {
            render_typst(&report, &options)
        };
        for warning in result.warnings {
            eprintln!("warning: {warning}");
        }
        fs::write(args.output, result.text)?;
    }
    Ok(())
}
fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = parse(args.into_iter())
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
        .and_then(run);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
