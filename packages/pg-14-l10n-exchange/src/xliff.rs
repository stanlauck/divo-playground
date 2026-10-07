// SPDX-License-Identifier: MIT OR Apache-2.0

//! XLIFF 2.1 mapping.
//!
//! Each entry becomes one `<unit>` with one `<segment>`. Context, character
//! name and translator note become `<note>` elements with the categories
//! `context`, `character` and `comment`. `max_length` maps to the Size and
//! Length Restriction module with the `xliff:codepoints` profile.

use std::fmt::Write as _;

use roxmltree::{Document, Node, ParsingOptions};

use crate::error::{Error, Result};
use crate::model::{Entry, SCHEMA_VERSION, State, StringTable, none_if_empty};

/// XLIFF 2.x core namespace (2.1 keeps the 2.0 URN).
pub const XLIFF_NS: &str = "urn:oasis:names:tc:xliff:document:2.0";
/// Size and Length Restriction module namespace.
pub const SLR_NS: &str = "urn:oasis:names:tc:xliff:sizerestriction:2.0";
const CODEPOINTS: &str = "xliff:codepoints";
const FILE_ID: &str = "f1";

/// Serializes a table as an XLIFF 2.1 document with a single `<file>`.
pub fn to_xliff(table: &StringTable) -> Result<String> {
    table.validate()?;
    let uses_slr = table.strings.iter().any(|e| e.max_length.is_some());

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    write!(out, "<xliff xmlns=\"{XLIFF_NS}\"").unwrap();
    if uses_slr {
        write!(out, " xmlns:slr=\"{SLR_NS}\"").unwrap();
    }
    write!(
        out,
        " version=\"2.1\" srcLang=\"{}\"",
        attr(&table.source_lang)
    )
    .unwrap();
    if let Some(lang) = &table.target_lang {
        write!(out, " trgLang=\"{}\"", attr(lang)).unwrap();
    }
    out.push_str(">\n");
    writeln!(out, "  <file id=\"{FILE_ID}\" xml:space=\"preserve\">").unwrap();
    if uses_slr {
        writeln!(out, "    <slr:profiles generalProfile=\"{CODEPOINTS}\"/>").unwrap();
    }
    for entry in &table.strings {
        write_unit(&mut out, entry);
    }
    out.push_str("  </file>\n</xliff>\n");
    Ok(out)
}

fn write_unit(out: &mut String, e: &Entry) {
    write!(out, "    <unit id=\"{}\"", attr(&e.id)).unwrap();
    if let Some(max) = e.max_length {
        write!(out, " slr:sizeRestriction=\"{max}\"").unwrap();
    }
    out.push_str(">\n");

    let notes = [
        ("context", &e.context),
        ("character", &e.character),
        ("comment", &e.note),
    ];
    if notes.iter().any(|(_, v)| v.is_some()) {
        out.push_str("      <notes>\n");
        for (category, value) in notes {
            if let Some(v) = value {
                writeln!(
                    out,
                    "        <note category=\"{category}\">{}</note>",
                    text(v)
                )
                .unwrap();
            }
        }
        out.push_str("      </notes>\n");
    }

    out.push_str("      <segment");
    if let Some(state) = e.state {
        write!(out, " state=\"{state}\"").unwrap();
    }
    out.push_str(">\n");
    writeln!(out, "        <source>{}</source>", text(&e.source)).unwrap();
    if let Some(t) = &e.target {
        writeln!(out, "        <target>{}</target>", text(t)).unwrap();
    }
    out.push_str("      </segment>\n    </unit>\n");
}

fn text(s: &str) -> String {
    escape(s, false)
}

fn attr(s: &str) -> String {
    escape(s, true)
}

// Carriage returns are written as references because XML parsers normalize
// raw CR/CRLF to LF; in attributes the same applies to LF and TAB.
fn escape(s: &str, in_attr: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            '"' if in_attr => out.push_str("&quot;"),
            '\n' if in_attr => out.push_str("&#10;"),
            '\t' if in_attr => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

/// Parses an XLIFF 2.0 or 2.1 document into a string table.
///
/// Units nested in `<group>` and spread over several `<file>` elements are
/// flattened in document order. A unit split into several segments is joined
/// back into one entry. Inline markup (`<ph>`, `<pc>`, `<mrk>`, ...) is
/// rejected because the string table model is plain text.
pub fn from_xliff(xml: &str) -> Result<StringTable> {
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: false,
            ..ParsingOptions::default()
        },
    )?;
    let root = doc.root_element();
    if !is(root, XLIFF_NS, "xliff") {
        return Err(Error::Invalid(
            "root element is not an XLIFF 2 <xliff>".into(),
        ));
    }
    match root.attribute("version") {
        Some("2.0" | "2.1") => {}
        other => {
            return Err(Error::Unsupported(format!("XLIFF version {other:?}")));
        }
    }
    let source_lang = root
        .attribute("srcLang")
        .ok_or_else(|| Error::Invalid("<xliff> has no srcLang".into()))?;

    let mut table = StringTable {
        version: SCHEMA_VERSION,
        source_lang: source_lang.to_owned(),
        target_lang: none_if_empty(root.attribute("trgLang").map(str::to_owned)),
        strings: Vec::new(),
    };
    for file in root.children().filter(|n| is(*n, XLIFF_NS, "file")) {
        check_profile(file)?;
        collect_units(file, &mut table.strings)?;
    }
    table.validate()?;
    Ok(table)
}

fn collect_units(parent: Node, out: &mut Vec<Entry>) -> Result<()> {
    for child in parent.children().filter(Node::is_element) {
        if is(child, XLIFF_NS, "group") {
            check_profile(child)?;
            collect_units(child, out)?;
        } else if is(child, XLIFF_NS, "unit") {
            check_profile(child)?;
            out.push(read_unit(child)?);
        }
    }
    Ok(())
}

// Only code point limits fit `max_length`; other profiles count differently.
fn check_profile(node: Node) -> Result<()> {
    let profiles = node.children().find(|n| is(*n, SLR_NS, "profiles"));
    match profiles.and_then(|p| p.attribute("generalProfile")) {
        None | Some(CODEPOINTS) => Ok(()),
        Some(other) => Err(Error::Unsupported(format!(
            "size restriction profile {other:?} (only {CODEPOINTS} is mapped)"
        ))),
    }
}

fn read_unit(unit: Node) -> Result<Entry> {
    let id = unit
        .attribute("id")
        .ok_or_else(|| Error::Invalid("<unit> without id".into()))?;
    let mut entry = Entry::new(id, "");

    if let Some(raw) = unit.attribute((SLR_NS, "sizeRestriction")) {
        let max = raw.trim().parse::<u32>().map_err(|_| {
            Error::Unsupported(format!(
                "unit {id:?}: sizeRestriction {raw:?} is not a single limit"
            ))
        })?;
        entry.max_length = Some(max);
    }

    let (mut context, mut character, mut note) = (Vec::new(), Vec::new(), Vec::new());
    if let Some(notes) = unit.children().find(|n| is(*n, XLIFF_NS, "notes")) {
        for n in notes.children().filter(|n| is(*n, XLIFF_NS, "note")) {
            let body = plain_text(n, id)?;
            match n.attribute("category") {
                Some("context") => context.push(body),
                Some("character") => character.push(body),
                _ => note.push(body),
            }
        }
    }
    entry.context = none_if_empty(Some(context.join("\n")));
    entry.character = none_if_empty(Some(character.join("\n")));
    entry.note = none_if_empty(Some(note.join("\n")));

    let mut source = String::new();
    let mut target = String::new();
    let mut has_target = true;
    let mut states: Vec<Option<State>> = Vec::new();
    for part in unit.children().filter(Node::is_element) {
        let is_segment = is(part, XLIFF_NS, "segment");
        if !is_segment && !is(part, XLIFF_NS, "ignorable") {
            continue;
        }
        let src = part
            .children()
            .find(|n| is(*n, XLIFF_NS, "source"))
            .ok_or_else(|| Error::Invalid(format!("unit {id:?}: segment without <source>")))?;
        let src_text = plain_text(src, id)?;
        source.push_str(&src_text);

        match part.children().find(|n| is(*n, XLIFF_NS, "target")) {
            Some(t) => target.push_str(&plain_text(t, id)?),
            // An ignorable without a target carries its source text over.
            None if !is_segment => target.push_str(&src_text),
            None => has_target = false,
        }

        if is_segment {
            states.push(part.attribute("state").map(str::parse).transpose()?);
        }
    }
    if states.is_empty() {
        return Err(Error::Invalid(format!("unit {id:?} has no <segment>")));
    }
    entry.source = source;
    entry.target = if has_target {
        none_if_empty(Some(target))
    } else {
        None
    };
    entry.state = merge_states(&states);
    Ok(entry)
}

/// A joined entry is only as far along as its least advanced segment.
/// A missing state means `initial` (the XLIFF default) once segments disagree.
fn merge_states(states: &[Option<State>]) -> Option<State> {
    if states.iter().all(Option::is_none) {
        return None;
    }
    states
        .iter()
        .map(|s| s.unwrap_or(State::Initial))
        .min_by_key(|s| match s {
            State::Initial => 0,
            State::Translated => 1,
            State::Reviewed => 2,
            State::Final => 3,
        })
}

fn plain_text(node: Node, unit_id: &str) -> Result<String> {
    let mut out = String::new();
    for child in node.children() {
        if child.is_element() {
            return Err(Error::Unsupported(format!(
                "unit {unit_id:?}: inline element <{}> inside <{}>",
                child.tag_name().name(),
                node.tag_name().name()
            )));
        }
        // Comments and processing instructions are not part of the text.
        if child.is_text() {
            out.push_str(child.text().unwrap_or_default());
        }
    }
    Ok(out)
}

fn is(node: Node, ns: &str, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(ns) && node.tag_name().name() == name
}
