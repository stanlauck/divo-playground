// SPDX-License-Identifier: MIT OR Apache-2.0

//! A small self-contained Fountain parser covering the elements that take
//! space on the page. Sections, synopses, notes and boneyard are dropped.
//! The element rules follow the public Fountain syntax reference
//! (<https://fountain.io/syntax>); see the README for the exact subset.

use crate::error::{Error, Result};

/// Maximum accepted input size in bytes (16 MiB).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum accepted number of input lines.
pub const MAX_INPUT_LINES: usize = 1_000_000;

/// One `key: value` entry of the title page. Multi-line values are joined
/// with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleEntry {
    pub key: String,
    pub value: String,
}

/// One line inside a dialogue block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueLine {
    /// A `(wrylie)` line.
    Parenthetical(String),
    /// Ordinary spoken text. An empty string is an intentional blank line
    /// (a line of two or more spaces in the source).
    Text(String),
    /// A `~lyric` line.
    Lyric(String),
}

/// A character cue with its dialogue lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueBlock {
    /// Character name including any extension such as `(V.O.)`.
    pub character: String,
    /// `true` when the cue ended with `^` (right column of dual dialogue).
    pub dual: bool,
    pub lines: Vec<DialogueLine>,
}

/// A body element that occupies space on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    SceneHeading {
        /// Heading text without the forcing `.` and without the scene number.
        text: String,
        /// Scene number from a trailing `#number#`, if any.
        number: Option<String>,
    },
    /// One action paragraph; each source line is kept as its own entry
    /// (leading indentation preserved, tabs expanded to four spaces).
    Action(Vec<String>),
    /// A paragraph whose first line is `>text<`; each line already stripped.
    Centered(Vec<String>),
    /// A paragraph of `~` lyric lines outside dialogue.
    Lyrics(Vec<String>),
    Dialogue(DialogueBlock),
    Transition(String),
    /// A `===` line.
    PageBreak,
}

/// A parsed Fountain document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub title_page: Vec<TitleEntry>,
    pub elements: Vec<Element>,
}

impl Document {
    /// `true` when the document has a title page.
    pub fn has_title_page(&self) -> bool {
        !self.title_page.is_empty()
    }
}

/// Parses Fountain text into layout-relevant elements.
pub fn parse(text: &str) -> Result<Document> {
    if text.len() > MAX_INPUT_BYTES {
        return Err(Error::InputTooLarge {
            bytes: text.len(),
            max: MAX_INPUT_BYTES,
        });
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let line_count = text.split('\n').count();
    if line_count > MAX_INPUT_LINES {
        return Err(Error::TooManyLines {
            lines: line_count,
            max: MAX_INPUT_LINES,
        });
    }
    let raw: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let lines = strip_comments(&raw);
    let (title_page, body_start) = parse_title_page(&lines);
    let elements = parse_body(&lines[body_start..]);
    Ok(Document {
        title_page,
        elements,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Skip {
    None,
    Boneyard,
    Note,
}

/// Removes `/* boneyard */` and `[[ notes ]]` (both may span lines). A line
/// that contained nothing but removed text disappears entirely.
fn strip_comments(lines: &[&str]) -> Vec<String> {
    let mut state = Skip::None;
    let mut out = Vec::with_capacity(lines.len());
    for raw in lines {
        let had_text = !raw.trim().is_empty();
        let mut kept = String::new();
        let mut rest: &str = raw;
        loop {
            match state {
                Skip::None => {
                    let boneyard = rest.find("/*");
                    let note = rest.find("[[");
                    let next = match (boneyard, note) {
                        (Some(b), Some(n)) if b <= n => Some((b, Skip::Boneyard)),
                        (Some(_), Some(n)) => Some((n, Skip::Note)),
                        (Some(b), None) => Some((b, Skip::Boneyard)),
                        (None, Some(n)) => Some((n, Skip::Note)),
                        (None, None) => None,
                    };
                    match next {
                        None => {
                            kept.push_str(rest);
                            break;
                        }
                        Some((pos, kind)) => {
                            kept.push_str(&rest[..pos]);
                            rest = &rest[pos + 2..];
                            state = kind;
                        }
                    }
                }
                Skip::Boneyard => match rest.find("*/") {
                    Some(pos) => {
                        rest = &rest[pos + 2..];
                        state = Skip::None;
                    }
                    None => break,
                },
                Skip::Note => match rest.find("]]") {
                    Some(pos) => {
                        rest = &rest[pos + 2..];
                        state = Skip::None;
                    }
                    None => break,
                },
            }
        }
        if had_text && kept.trim().is_empty() {
            continue;
        }
        out.push(kept);
    }
    out
}

/// A hard blank line separates paragraphs. A whitespace-only line of two or
/// more characters is a "soft" blank that stays inside its paragraph.
fn is_hard_blank(line: &str) -> bool {
    line.trim().is_empty() && line.chars().count() < 2
}

fn is_soft_blank(line: &str) -> bool {
    line.trim().is_empty() && !is_hard_blank(line)
}

fn title_key(line: &str) -> Option<&str> {
    if line.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-')
        || !key.chars().any(|c| c.is_alphabetic())
    {
        return None;
    }
    Some(key)
}

/// Keys that may open a title page. Later entries in the same block may use
/// any key; only the first one is checked, so an action paragraph such as
/// `Note: she leaves.` at the very top is not mistaken for a title page.
const TITLE_KEYS: [&str; 12] = [
    "title",
    "credit",
    "author",
    "authors",
    "source",
    "draft date",
    "date",
    "contact",
    "copyright",
    "notes",
    "revision",
    "format",
];

fn is_known_title_key(key: &str) -> bool {
    let lower = key.trim().to_lowercase();
    TITLE_KEYS.contains(&lower.as_str())
}

fn is_title_continuation(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("   ")
}

/// Parses the title page: the first block of lines when its first line is a
/// `key:` entry. Returns the entries and the index of the first body line.
fn parse_title_page(lines: &[String]) -> (Vec<TitleEntry>, usize) {
    let first = match lines.iter().position(|l| !l.trim().is_empty()) {
        Some(i) => i,
        None => return (Vec::new(), lines.len()),
    };
    match title_key(&lines[first]) {
        Some(key) if is_known_title_key(key) => {}
        _ => return (Vec::new(), 0),
    }
    let mut entries: Vec<TitleEntry> = Vec::new();
    let mut i = first;
    while i < lines.len() && !is_hard_blank(&lines[i]) {
        let line = &lines[i];
        if let Some(key) = title_key(line) {
            let value = line[key.len() + 1..].trim().to_string();
            entries.push(TitleEntry {
                key: key.trim().to_string(),
                value,
            });
        } else if is_title_continuation(line) {
            if let Some(last) = entries.last_mut() {
                let piece = line.trim();
                if !last.value.is_empty() {
                    last.value.push('\n');
                }
                last.value.push_str(piece);
            }
        } else {
            // Not a title-page line: the title page ends here.
            break;
        }
        i += 1;
    }
    (entries, i)
}

fn parse_body(lines: &[String]) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if is_hard_blank(line) {
            flush_paragraph(&mut paragraph, &mut elements);
            continue;
        }
        if is_page_break(trimmed) {
            flush_paragraph(&mut paragraph, &mut elements);
            elements.push(Element::PageBreak);
            continue;
        }
        if is_section_or_synopsis(line) {
            continue;
        }
        paragraph.push(line);
    }
    flush_paragraph(&mut paragraph, &mut elements);
    elements
}

fn is_page_break(trimmed: &str) -> bool {
    trimmed.len() >= 3 && trimmed.bytes().all(|b| b == b'=')
}

fn is_section_or_synopsis(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#') || t.starts_with('=')
}

fn flush_paragraph(paragraph: &mut Vec<&str>, elements: &mut Vec<Element>) {
    if paragraph.is_empty() {
        return;
    }
    classify_paragraph(paragraph, elements);
    paragraph.clear();
}

fn classify_paragraph(paragraph: &[&str], elements: &mut Vec<Element>) {
    let first = paragraph[0];
    let trimmed = first.trim();

    if let Some(rest) = trimmed.strip_prefix('!') {
        let mut lines = vec![expand_action_line(rest)];
        lines.extend(paragraph[1..].iter().map(|l| expand_action_line(l)));
        elements.push(Element::Action(lines));
        return;
    }
    if let Some(heading) = forced_scene_heading(trimmed).or_else(|| natural_scene_heading(trimmed))
    {
        let (text, number) = split_scene_number(heading);
        elements.push(Element::SceneHeading { text, number });
        push_action_rest(&paragraph[1..], elements);
        return;
    }
    if is_centered(trimmed) {
        let lines = paragraph
            .iter()
            .map(|l| strip_centered(l.trim()).to_string())
            .collect();
        elements.push(Element::Centered(lines));
        return;
    }
    if let Some(rest) = trimmed.strip_prefix('>') {
        elements.push(Element::Transition(rest.trim().to_string()));
        push_action_rest(&paragraph[1..], elements);
        return;
    }
    if paragraph.len() == 1 && is_natural_transition(trimmed) {
        elements.push(Element::Transition(trimmed.to_string()));
        return;
    }
    if paragraph.len() >= 2 {
        if let Some((character, dual)) = character_cue(trimmed) {
            let lines = paragraph[1..].iter().map(|l| dialogue_line(l)).collect();
            elements.push(Element::Dialogue(DialogueBlock {
                character,
                dual,
                lines,
            }));
            return;
        }
    }
    if trimmed.starts_with('~') {
        let lines = paragraph
            .iter()
            .map(|l| {
                l.trim()
                    .strip_prefix('~')
                    .unwrap_or(l.trim())
                    .trim()
                    .to_string()
            })
            .collect();
        elements.push(Element::Lyrics(lines));
        return;
    }
    elements.push(Element::Action(
        paragraph.iter().map(|l| expand_action_line(l)).collect(),
    ));
}

fn push_action_rest(rest: &[&str], elements: &mut Vec<Element>) {
    if !rest.is_empty() {
        elements.push(Element::Action(
            rest.iter().map(|l| expand_action_line(l)).collect(),
        ));
    }
}

/// Keeps leading indentation (tabs become four spaces), trims the end.
fn expand_action_line(line: &str) -> String {
    let body = line.trim_end();
    let indent_len = body.len() - body.trim_start().len();
    let mut out = String::with_capacity(body.len() + 8);
    for c in body[..indent_len].chars() {
        if c == '\t' {
            out.push_str("    ");
        } else {
            out.push(' ');
        }
    }
    out.push_str(&body[indent_len..]);
    out
}

fn forced_scene_heading(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix('.')?;
    if rest.starts_with(|c: char| c.is_alphanumeric()) {
        Some(rest.trim())
    } else {
        None
    }
}

const HEADING_PREFIXES: [&str; 6] = ["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"];

fn natural_scene_heading(trimmed: &str) -> Option<&str> {
    let upper = trimmed.to_uppercase();
    for prefix in HEADING_PREFIXES {
        if let Some(after) = upper.strip_prefix(prefix) {
            if after.starts_with('.') || after.starts_with(' ') {
                return Some(trimmed);
            }
        }
    }
    None
}

/// Splits a trailing `#number#` off a heading.
fn split_scene_number(heading: &str) -> (String, Option<String>) {
    let text = heading.trim();
    if let Some(without_last) = text.strip_suffix('#') {
        if let Some(open) = without_last.rfind('#') {
            let number = &without_last[open + 1..];
            if !number.is_empty()
                && number
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
            {
                return (
                    without_last[..open].trim_end().to_string(),
                    Some(number.to_string()),
                );
            }
        }
    }
    (text.to_string(), None)
}

fn is_centered(trimmed: &str) -> bool {
    trimmed.len() >= 2 && trimmed.starts_with('>') && trimmed.ends_with('<')
}

fn strip_centered(trimmed: &str) -> &str {
    if is_centered(trimmed) {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    }
}

fn has_letter(s: &str) -> bool {
    s.chars().any(char::is_alphabetic)
}

fn has_lowercase(s: &str) -> bool {
    s.chars().any(char::is_lowercase)
}

fn is_natural_transition(trimmed: &str) -> bool {
    trimmed.ends_with("TO:") && has_letter(trimmed) && !has_lowercase(trimmed)
}

/// Returns the character name (with extensions) and the dual-dialogue flag.
fn character_cue(trimmed: &str) -> Option<(String, bool)> {
    let (body, dual) = match trimmed.strip_suffix('^') {
        Some(b) => (b.trim_end(), true),
        None => (trimmed, false),
    };
    if let Some(forced) = body.strip_prefix('@') {
        let name = forced.trim();
        if name.is_empty() {
            return None;
        }
        return Some((name.to_string(), dual));
    }
    // Only the text outside parentheses must be uppercase.
    let mut depth = 0usize;
    let mut outside = String::new();
    for c in body.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => outside.push(c),
            _ => {}
        }
    }
    if has_letter(&outside) && !has_lowercase(&outside) {
        Some((body.to_string(), dual))
    } else {
        None
    }
}

fn dialogue_line(line: &str) -> DialogueLine {
    if is_soft_blank(line) {
        return DialogueLine::Text(String::new());
    }
    let t = line.trim();
    if t.len() >= 2 && t.starts_with('(') && t.ends_with(')') {
        DialogueLine::Parenthetical(t.to_string())
    } else if let Some(lyric) = t.strip_prefix('~') {
        DialogueLine::Lyric(lyric.trim().to_string())
    } else {
        DialogueLine::Text(t.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(text: &str) -> Vec<Element> {
        parse(text).unwrap().elements
    }

    #[test]
    fn title_page_is_split_from_body() {
        let doc =
            parse("Title: Synthetic\nAuthor: Nobody\n   Second line\n\nINT. ROOM - DAY\n").unwrap();
        assert_eq!(doc.title_page.len(), 2);
        assert_eq!(doc.title_page[0].key, "Title");
        assert_eq!(doc.title_page[1].value, "Nobody\nSecond line");
        assert_eq!(
            doc.elements,
            vec![Element::SceneHeading {
                text: "INT. ROOM - DAY".into(),
                number: None
            }]
        );
    }

    #[test]
    fn no_title_page_when_first_line_is_not_a_key() {
        let doc = parse("INT. ROOM - DAY\n\nHello.\n").unwrap();
        assert!(!doc.has_title_page());
        assert_eq!(doc.elements.len(), 2);
    }

    #[test]
    fn scene_heading_prefixes_and_numbers() {
        for h in [
            "INT. A",
            "ext. b",
            "EST. C",
            "INT./EXT. D",
            "I/E. E",
            "INT/EXT F",
        ] {
            assert!(
                matches!(body(h).as_slice(), [Element::SceneHeading { .. }]),
                "{h}"
            );
        }
        assert!(matches!(body("INTERIOR").as_slice(), [Element::Action(_)]));
        assert_eq!(
            body("EXT. BEACH - NIGHT #12A#"),
            vec![Element::SceneHeading {
                text: "EXT. BEACH - NIGHT".into(),
                number: Some("12A".into())
            }]
        );
    }

    #[test]
    fn forced_heading_and_forced_action() {
        assert_eq!(
            body(".CELLAR"),
            vec![Element::SceneHeading {
                text: "CELLAR".into(),
                number: None
            }]
        );
        assert!(matches!(body("...pause").as_slice(), [Element::Action(_)]));
        assert_eq!(
            body("!INT. NOT A HEADING"),
            vec![Element::Action(vec!["INT. NOT A HEADING".into()])]
        );
    }

    #[test]
    fn character_with_dialogue_parenthetical_and_lyric() {
        let got = body("ANNA (V.O.)\n(softly)\nHello there.\n~la la\n");
        assert_eq!(
            got,
            vec![Element::Dialogue(DialogueBlock {
                character: "ANNA (V.O.)".into(),
                dual: false,
                lines: vec![
                    DialogueLine::Parenthetical("(softly)".into()),
                    DialogueLine::Text("Hello there.".into()),
                    DialogueLine::Lyric("la la".into()),
                ]
            })]
        );
    }

    #[test]
    fn uppercase_single_line_is_action_not_character() {
        assert!(matches!(body("BANG!").as_slice(), [Element::Action(_)]));
    }

    #[test]
    fn forced_character_and_dual_marker() {
        let got = body("@McLEOD\nHi.\n\nBOB ^\nHo.\n");
        assert!(matches!(
            &got[0],
            Element::Dialogue(DialogueBlock { character, dual: false, .. }) if character == "McLEOD"
        ));
        assert!(matches!(
            &got[1],
            Element::Dialogue(DialogueBlock { character, dual: true, .. }) if character == "BOB"
        ));
    }

    #[test]
    fn two_space_line_keeps_dialogue_together() {
        let got = body("BOB\nOne.\n  \nTwo.\n");
        assert_eq!(got.len(), 1);
        if let Element::Dialogue(block) = &got[0] {
            assert_eq!(block.lines.len(), 3);
            assert_eq!(block.lines[1], DialogueLine::Text(String::new()));
        } else {
            panic!("expected dialogue");
        }
    }

    #[test]
    fn transitions_natural_and_forced() {
        assert_eq!(body("CUT TO:"), vec![Element::Transition("CUT TO:".into())]);
        assert_eq!(
            body("> fade out"),
            vec![Element::Transition("fade out".into())]
        );
        assert!(matches!(body("cut to:").as_slice(), [Element::Action(_)]));
    }

    #[test]
    fn centered_text() {
        assert_eq!(
            body("> THE END <"),
            vec![Element::Centered(vec!["THE END".into()])]
        );
    }

    #[test]
    fn lyrics_outside_dialogue() {
        assert_eq!(
            body("~ row row\n~ your boat"),
            vec![Element::Lyrics(vec!["row row".into(), "your boat".into()])]
        );
    }

    #[test]
    fn page_break_sections_synopses_notes_boneyard() {
        let got = body(
            "# Act One\n= The setup.\nINT. A - DAY\n\n===\n\nText [[a note]] here.\n\n/* gone\nstill gone */\nBack.\n[[own line]]\n",
        );
        assert_eq!(
            got,
            vec![
                Element::SceneHeading {
                    text: "INT. A - DAY".into(),
                    number: None
                },
                Element::PageBreak,
                Element::Action(vec!["Text  here.".into()]),
                Element::Action(vec!["Back.".into()]),
            ]
        );
    }

    #[test]
    fn crlf_and_bom_are_normalised() {
        let got = body("\u{feff}INT. A - DAY\r\n\r\nHi.\r\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[1], Element::Action(vec!["Hi.".into()]));
    }

    #[test]
    fn action_keeps_indentation_and_expands_tabs() {
        assert_eq!(
            body("\tIndented."),
            vec![Element::Action(vec!["    Indented.".into()])]
        );
    }

    #[test]
    fn limits_are_enforced() {
        let big = "a".repeat(MAX_INPUT_BYTES + 1);
        assert!(matches!(parse(&big), Err(Error::InputTooLarge { .. })));
        let many = "\n".repeat(MAX_INPUT_LINES);
        assert!(matches!(parse(&many), Err(Error::TooManyLines { .. })));
    }

    #[test]
    fn empty_and_whitespace_inputs_parse_to_nothing() {
        assert_eq!(parse("").unwrap(), Document::default());
        assert_eq!(parse("\n\n   \n").unwrap().elements, Vec::new());
    }
}
