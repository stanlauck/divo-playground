// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{validate, Block, CalloutKind, Report, Section};
use std::{
    collections::HashSet,
    fmt::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageSize {
    #[default]
    A4,
    Letter,
}

/// Include is additive: explicitly included IDs enable optional sections.
/// All omitted parents suppress descendants. Exclude always wins.
#[derive(Clone, Debug, Default)]
pub struct RenderOptions {
    pub include: Option<Vec<String>>,
    pub exclude: Vec<String>,
    pub include_optional: bool,
    pub toc: bool,
    pub page: PageSize,
    /// Typst image resolution root, default current directory. Markdown ignores it.
    pub image_root: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    UnknownSection { id: String, option: &'static str },
    MissingImage { path: String },
    InvalidReport { message: String },
}
impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSection { id, option } => write!(f, "unknown section {id:?} in {option}"),
            Self::MissingImage { path } => write!(
                f,
                "missing or inaccessible image {path:?}; using placeholder"
            ),
            Self::InvalidReport { message } => write!(f, "{message}"),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub warnings: Vec<Warning>,
}

fn select<'a>(report: &'a Report, options: &RenderOptions) -> (Vec<&'a Section>, Vec<Warning>) {
    let mut all = HashSet::new();
    let mut stack: Vec<_> = report.sections.iter().rev().collect();
    while let Some(s) = stack.pop() {
        all.insert(s.id.as_str());
        stack.extend(s.sections.iter().rev());
    }
    let mut warnings = Vec::new();
    for (option, ids) in [
        ("include", options.include.as_deref().unwrap_or(&[])),
        ("exclude", options.exclude.as_slice()),
    ] {
        let mut seen = HashSet::new();
        for id in ids {
            if !all.contains(id.as_str()) && seen.insert(id) {
                warnings.push(Warning::UnknownSection {
                    id: id.clone(),
                    option,
                });
            }
        }
    }
    let include: HashSet<_> = options
        .include
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .collect();
    let exclude: HashSet<_> = options.exclude.iter().map(String::as_str).collect();
    let mut selected = Vec::new();
    stack = report.sections.iter().rev().collect();
    while let Some(s) = stack.pop() {
        if !s.enabled
            || exclude.contains(s.id.as_str())
            || (s.optional && !options.include_optional && !include.contains(s.id.as_str()))
        {
            continue;
        }
        selected.push(s);
        stack.extend(s.sections.iter().rev());
    }
    (selected, warnings)
}
fn invalid(report: &Report) -> Option<Rendered> {
    validate(report).err().map(|e| Rendered {
        text: String::new(),
        warnings: vec![Warning::InvalidReport {
            message: e.to_string(),
        }],
    })
}
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// CommonMark literal text; only renderer-generated markup is interpreted.
/// GFM cells and headings use generated <br> for embedded line breaks.
///
/// Escaping is context-aware so the Markdown stays readable: characters that
/// can open inline constructs anywhere (`\ * _ ` [ ] < > | ~ & #`) are always
/// escaped; block starters are escaped only at the start of a line (`- + > =`
/// and a leading number followed by `.` or `)`); everything else (`.`, `:`,
/// `,`, `-` inside words, quotes, parentheses) is left as written.
fn md(s: &str) -> String {
    const ALWAYS: &str = "\\*_`[]<>|~&#";
    const LINE_START: &str = "-+>=";
    let mut out = String::new();
    for (line_index, line) in lf(s).split('\n').enumerate() {
        if line_index > 0 {
            out.push_str("<br>");
        }
        let body = line.trim_start_matches(' ');
        for _ in 0..line.len() - body.len() {
            out.push_str("&#32;");
        }
        let mut first = true;
        let mut digits_only = true;
        let mut saw_digit = false;
        for c in body.chars() {
            if c == '\t' {
                out.push_str("&#9;");
            } else if c.is_control() {
                let _ = write!(out, "&#{};", c as u32);
            } else {
                let block_start = first && LINE_START.contains(c);
                let ordered = digits_only && saw_digit && (c == '.' || c == ')');
                if ALWAYS.contains(c) || block_start || ordered {
                    out.push('\\');
                }
                out.push(c);
            }
            first = false;
            if c.is_ascii_digit() {
                saw_digit = true;
            } else {
                digits_only = false;
            }
        }
    }
    out
}
fn destination(path: &str) -> String {
    let mut s = String::new();
    // URI encoding protects the Markdown link grammar without touching the file.
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            s.push(b as char);
        } else {
            let _ = write!(s, "%{b:02X}");
        }
    }
    s
}
fn md_table(out: &mut String, columns: &[String], rows: &[Vec<String>]) {
    out.push('|');
    for cell in columns {
        let _ = write!(out, " {} |", md(cell));
    }
    out.push('\n');
    out.push('|');
    for _ in columns {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows {
        out.push('|');
        for cell in row {
            let _ = write!(out, " {} |", md(cell));
        }
        out.push('\n');
    }
    out.push('\n');
}
fn md_block(out: &mut String, block: &Block) {
    match block {
        Block::Paragraph { text } => {
            let _ = writeln!(out, "{}\n", md(text));
        }
        Block::Heading { text, level } => {
            let _ = writeln!(out, "{} {}\n", "#".repeat(*level as usize), md(text));
        }
        Block::Bullets { items } | Block::Numbered { items } => {
            for (i, item) in items.iter().enumerate() {
                if matches!(block, Block::Numbered { .. }) {
                    let _ = writeln!(out, "{}. {}", i + 1, md(item));
                } else {
                    let _ = writeln!(out, "- {}", md(item));
                }
            }
            if !items.is_empty() {
                out.push('\n');
            }
        }
        Block::Table {
            columns,
            rows,
            caption,
        } => {
            md_table(out, columns, rows);
            if let Some(caption) = caption {
                let _ = writeln!(out, "{}\n", md(caption));
            }
        }
        Block::KeyValues { pairs } => {
            md_table(
                out,
                &["Key".into(), "Value".into()],
                &pairs
                    .iter()
                    .map(|(k, v)| vec![k.clone(), v.clone()])
                    .collect::<Vec<_>>(),
            );
        }
        Block::Code { text, language } => {
            let text = lf(text);
            let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(3.max(longest + 1));
            let _ = writeln!(out, "{fence}{}", language.as_deref().unwrap_or(""));
            out.push_str(&text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            let _ = writeln!(out, "{fence}\n");
        }
        Block::Quote { text, attribution } => {
            let _ = writeln!(out, "> {}", md(text));
            if let Some(a) = attribution {
                let _ = writeln!(out, ">\n> — {}", md(a));
            }
            out.push('\n');
        }
        Block::Callout { kind, text } => {
            let _ = writeln!(out, "> **{}:** {}\n", kind.label(), md(text));
        }
        Block::PageBreak {} => out.push_str("<!-- page break -->\n\n"),
        Block::Image { path, caption, .. } => {
            let _ = writeln!(
                out,
                "![{}]({})\n",
                md(caption.as_deref().unwrap_or("")),
                destination(path)
            );
        }
    }
}

/// Render deterministic LF Markdown. This function never accesses images.
/// Invalid programmatic models return empty text and an InvalidReport warning.
pub fn render_markdown(report: &Report, options: &RenderOptions) -> Rendered {
    if let Some(result) = invalid(report) {
        return result;
    }
    let (sections, warnings) = select(report, options);
    let mut text = format!("# {}\n\n", md(&report.title));
    for value in [&report.subtitle, &report.author, &report.date]
        .into_iter()
        .flatten()
    {
        let _ = writeln!(text, "{}\n", md(value));
    }
    if let Some(summary) = &report.summary {
        let _ = writeln!(text, "{}\n", md(summary));
    }
    if options.toc {
        text.push_str("## Contents\n\n");
        for (i, s) in sections.iter().enumerate() {
            let _ = writeln!(text, "- [{}](#section-{})", md(&s.title), i + 1);
        }
        text.push('\n');
    }
    for (i, section) in sections.iter().enumerate() {
        if options.toc {
            let _ = writeln!(text, "<a id=\"section-{}\"></a>\n", i + 1);
        }
        let _ = writeln!(
            text,
            "{} {}\n",
            "#".repeat(section.level as usize),
            md(&section.title)
        );
        for block in &section.blocks {
            md_block(&mut text, block);
        }
    }
    Rendered { text, warnings }
}

// Build Typst entirely with string literals inside function calls. Markup in
// text, paths, captions, and metadata can never become executable Typst.
fn ts(s: &str) -> String {
    let mut out = String::from("\"");
    for c in lf(s).chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn content(s: &str) -> String {
    format!("text({})", ts(s))
}
fn typ_table(out: &mut String, columns: &[String], rows: &[Vec<String>]) {
    let _ = writeln!(
        out,
        "#table(columns: {}, inset: 7pt, stroke: 0.5pt + rgb(\"d4dce5\"),",
        columns.len()
    );
    out.push_str("  table.header(");
    for c in columns {
        let _ = write!(out, "strong({}),", content(c));
    }
    out.push_str("),\n");
    for row in rows {
        out.push_str("  ");
        for c in row {
            let _ = write!(out, "{},", content(c));
        }
        out.push('\n');
    }
    out.push_str(")\n\n");
}

/// Resolve regular image files contained within a root (including symlinks).
pub(crate) fn image_file(root: &Path, path: &str) -> Option<PathBuf> {
    let base = root.canonicalize().ok()?;
    let file = root.join(path).canonicalize().ok()?;
    (file.starts_with(base) && file.is_file()).then_some(file)
}
fn typ_block(
    out: &mut String,
    block: &Block,
    options: &RenderOptions,
    warnings: &mut Vec<Warning>,
) {
    match block {
        Block::Paragraph { text } => {
            let _ = writeln!(out, "#{}\n", content(text));
        }
        Block::Heading { text, level } => {
            let _ = writeln!(
                out,
                "#heading(level: {level}, outlined: false, {})\n",
                content(text)
            );
        }
        Block::Bullets { items } | Block::Numbered { items } => {
            if !items.is_empty() {
                out.push_str(if matches!(block, Block::Bullets { .. }) {
                    "#list("
                } else {
                    "#enum("
                });
                for item in items {
                    let _ = write!(out, "{},", content(item));
                }
                out.push_str(")\n\n");
            }
        }
        Block::Table {
            columns,
            rows,
            caption,
        } => {
            typ_table(out, columns, rows);
            if let Some(caption) = caption {
                let _ = writeln!(out, "#text(size: 9pt, {})\n", ts(caption));
            }
        }
        Block::KeyValues { pairs } => typ_table(
            out,
            &["Key".into(), "Value".into()],
            &pairs
                .iter()
                .map(|(k, v)| vec![k.clone(), v.clone()])
                .collect::<Vec<_>>(),
        ),
        Block::Code { text, language } => {
            let _ = write!(out, "#raw({}, block: true", ts(text));
            if let Some(lang) = language {
                let _ = write!(out, ", lang: {}", ts(lang));
            }
            out.push_str(")\n\n");
        }
        Block::Quote { text, attribution } => {
            let _ = write!(out, "#quote(block: true, {}", content(text));
            if let Some(a) = attribution {
                let _ = write!(out, ", attribution: {}", content(a));
            }
            out.push_str(")\n\n");
        }
        Block::Callout { kind, text } => {
            let color = match kind {
                CalloutKind::Note => "e9f1fa",
                CalloutKind::Warning => "fff3d6",
                CalloutKind::Error => "fde8e8",
                CalloutKind::Success => "e6f4eb",
            };
            let _ = writeln!(out, "#block(width: 100%, inset: 10pt, radius: 4pt, fill: rgb(\"{color}\"))[#strong({}) #{}]\n", content(&format!("{}:", kind.label())), content(text));
        }
        Block::PageBreak {} => out.push_str("#pagebreak()\n\n"),
        Block::Image {
            path,
            caption,
            width_percent,
        } => {
            let width = width_percent.unwrap_or(100.0);
            if image_file(
                options.image_root.as_deref().unwrap_or(Path::new(".")),
                path,
            )
            .is_some()
            {
                let _ = writeln!(out, "#image({}, width: {width}%)\n", ts(path));
            } else {
                warnings.push(Warning::MissingImage { path: path.clone() });
                let _ = writeln!(out, "#block(width: {width}%, height: 48pt, inset: 8pt, stroke: 0.5pt + gray)[#{}]\n", content(&format!("Image unavailable: {path}")));
            }
            if let Some(caption) = caption {
                let _ = writeln!(out, "#text(size: 9pt, {})\n", ts(caption));
            }
        }
    }
}

/// Render a standalone Typst document. Checks selected images, never downloads.
pub fn render_typst(report: &Report, options: &RenderOptions) -> Rendered {
    if let Some(result) = invalid(report) {
        return result;
    }
    let (sections, mut warnings) = select(report, options);
    let mut text = String::from("// SPDX-License-Identifier: MIT OR Apache-2.0\n");
    let _ = writeln!(
        text,
        "#set page(paper: {}, margin: 20mm)",
        ts(match options.page {
            PageSize::A4 => "a4",
            PageSize::Letter => "us-letter",
        })
    );
    text.push_str("#set text(size: 10pt)\n#set par(leading: 0.7em)\n#set heading(numbering: \"1.1\")\n#set document(");
    let _ = write!(text, "title: {}", ts(&report.title));
    if let Some(author) = &report.author {
        let _ = write!(text, ", author: {}", ts(author));
    }
    // Typst otherwise inserts today's date into PDF metadata.
    if let Some(date) = &report.date {
        let _ = write!(
            text,
            ", date: datetime(year: {}, month: {}, day: {})",
            date[..4].parse::<u32>().unwrap_or(1),
            date[5..7].parse::<u32>().unwrap_or(1),
            date[8..].parse::<u32>().unwrap_or(1)
        );
    } else {
        text.push_str(", date: none");
    }
    text.push_str(")\n");
    if let Some(lang) = &report.language {
        // Typst's lang argument is an ISO language, not the entire BCP-47 tag.
        let base = lang.split('-').next().unwrap_or("en");
        if base.len() >= 2 {
            let _ = writeln!(text, "#set text(lang: {})", ts(&base.to_ascii_lowercase()));
        }
    }
    let _ = writeln!(
        text,
        "\n#text(size: 22pt, weight: \"bold\", {})\n",
        ts(&report.title)
    );
    if let Some(subtitle) = &report.subtitle {
        let _ = writeln!(text, "#text(size: 13pt, {})\n", ts(subtitle));
    }
    for value in [&report.author, &report.date].into_iter().flatten() {
        let _ = writeln!(text, "#{}\n", content(value));
    }
    if let Some(summary) = &report.summary {
        let _ = writeln!(text, "#{}\n", content(summary));
    }
    if options.toc {
        text.push_str("#outline(title: [Contents])\n\n");
    }
    for s in sections {
        let _ = writeln!(
            text,
            "#heading(level: {}, {})\n",
            s.level,
            content(&s.title)
        );
        for b in &s.blocks {
            typ_block(&mut text, b, options, &mut warnings);
        }
    }
    Rendered { text, warnings }
}
