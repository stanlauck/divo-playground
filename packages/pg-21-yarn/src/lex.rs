// SPDX-License-Identifier: MIT OR Apache-2.0

//! Line-level scanning: indentation, `//` comments, `#` hashtags, the
//! `Character:` speaker prefix, `[markup]` detection and `<<command>>` frames.
//!
//! Nothing here interprets meaning; it only splits text into the pieces the
//! block parser needs, keeping every byte offset so callers can report
//! character-accurate positions.

/// Columns a tab occupies when measuring option indentation.
pub(crate) const TAB_WIDTH: usize = 4;

/// One physical line with its 1-based number and terminator removed.
pub(crate) struct RawLine<'a> {
    pub number: u32,
    pub text: &'a str,
}

/// Splits on `\n`, stripping a trailing `\r` so LF and CRLF files agree.
pub(crate) fn split_lines(source: &str) -> Vec<RawLine<'_>> {
    let mut out = Vec::new();
    for (index, text) in source.split('\n').enumerate() {
        out.push(RawLine {
            number: index as u32 + 1,
            text: text.strip_suffix('\r').unwrap_or(text),
        });
    }
    // A trailing newline yields one empty final entry that is not a real line.
    if out.last().is_some_and(|line| line.text.is_empty()) && source.ends_with('\n') {
        out.pop();
    }
    out
}

/// Splits leading spaces and tabs from the rest of the line.
pub(crate) fn split_indent(raw: &str) -> (&str, &str) {
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
        index += 1;
    }
    raw.split_at(index)
}

pub(crate) fn indent_width(prefix: &str) -> usize {
    prefix
        .chars()
        .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

/// 1-based character column of `byte_offset` within `line`.
pub(crate) fn char_col(line: &str, byte_offset: usize) -> u32 {
    let bounded = byte_offset.min(line.len());
    line[..bounded].chars().count() as u32 + 1
}

/// A problem found while splitting hashtags off a line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScanIssue {
    /// A token in the hashtag region that does not start with `#`.
    MalformedHashtag,
    /// `#line:` with nothing after the colon.
    EmptyLineId,
    /// Two `#line:` hashtags on one line.
    DuplicateLineId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Scan {
    /// Text before the first hashtag or comment, with `\#` unescaped.
    pub text: String,
    pub line_id: Option<String>,
    pub tags: Vec<String>,
    /// Byte offsets into the scanned content, paired with the problem found.
    pub issues: Vec<(usize, ScanIssue)>,
}

/// Strips `//` comments and trailing `#hashtags` from one line of content.
///
/// Boundaries are only recognised at a token start (offset 0 or after
/// whitespace) and never inside `"strings"` or `{$interpolations}`, so text
/// such as `see http://example #note` keeps its URL and loses only the tag.
pub(crate) fn scan_line(content: &str) -> Scan {
    let bytes = content.as_bytes();
    let mut escaped_hashes = Vec::new();
    let mut index = 0;
    let mut hashtag_at = None;
    let mut comment_at = None;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => index = skip_string(bytes, index),
            b'{' if bytes.get(index + 1) == Some(&b'$') => {
                index = skip_interpolation(bytes, index);
            }
            b'\\' if bytes.get(index + 1) == Some(&b'#') => {
                escaped_hashes.push(index);
                index += 2;
            }
            b'#' if at_token_start(bytes, index) && names_a_hashtag(bytes, index) => {
                hashtag_at = Some(index);
                break;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') && at_token_start(bytes, index) => {
                comment_at = Some(index);
                break;
            }
            _ => index += 1,
        }
    }

    let mut scan = Scan::default();
    let text_end = hashtag_at.or(comment_at).unwrap_or(bytes.len());
    let mut text = content[..text_end].to_string();
    // Rewrite from the back so earlier offsets stay valid.
    for offset in escaped_hashes.iter().rev() {
        text.replace_range(*offset..*offset + 2, "#");
    }
    scan.text = text.trim_end().to_string();
    if comment_at.is_some() || hashtag_at.is_none() {
        return scan;
    }

    let start = hashtag_at.expect("checked above");
    let end = comment_in_region(bytes, start).unwrap_or(bytes.len());
    let mut cursor = start;
    while cursor < end {
        while cursor < end && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let token_start = cursor;
        while cursor < end && !bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if token_start == cursor {
            break;
        }
        record_hashtag(&mut scan, &content[token_start..cursor], token_start);
    }
    scan
}

fn record_hashtag(scan: &mut Scan, token: &str, offset: usize) {
    let Some(value) = token.strip_prefix('#') else {
        scan.issues.push((offset, ScanIssue::MalformedHashtag));
        return;
    };
    if let Some(id) = value.strip_prefix("line:") {
        if id.is_empty() {
            scan.issues.push((offset, ScanIssue::EmptyLineId));
        } else if scan.line_id.is_some() {
            scan.issues.push((offset, ScanIssue::DuplicateLineId));
        } else {
            scan.line_id = Some(id.to_string());
        }
        return;
    }
    if value.is_empty() {
        scan.issues.push((offset, ScanIssue::MalformedHashtag));
        return;
    }
    scan.tags.push(value.to_string());
}

fn at_token_start(bytes: &[u8], index: usize) -> bool {
    index == 0 || bytes[index - 1].is_ascii_whitespace()
}

/// A `#` only opens the hashtag region when it actually names a hashtag. A
/// bare `#` followed by whitespace is prose ("Press # to continue"), so it
/// stays in the line text instead of swallowing the rest of the line.
fn names_a_hashtag(bytes: &[u8], index: usize) -> bool {
    matches!(bytes.get(index + 1), Some(byte) if !byte.is_ascii_whitespace())
}

fn comment_in_region(bytes: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'/' && bytes[index + 1] == b'/' && at_token_start(bytes, index) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn skip_string(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn skip_interpolation(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 2;
    let mut depth = 1usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index = skip_string(bytes, index);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    bytes.len()
}

/// Splits a `Character:` speaker prefix off line or option text.
///
/// The name must be a single whitespace-free run before the first `:`, per the
/// Yarn Spinner 2 line rules, and must not look like markup, a command or an
/// interpolation. A colon followed by `/` or `\` is left alone so URLs in
/// dialogue survive.
pub(crate) fn split_speaker(text: &str) -> (Option<String>, String) {
    if text.starts_with("<<") {
        return (None, text.to_string());
    }
    let Some(index) = text.find(':') else {
        return (None, text.to_string());
    };
    let name = &text[..index];
    let rest = &text[index + 1..];
    if name.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
        return (None, text.to_string());
    }
    // A leading digit means this is prose like `10:30 in the morning`, not a
    // character name.
    let mut chars = name.chars();
    let valid = matches!(chars.next(), Some(first) if first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '\''));
    if !valid {
        return (None, text.to_string());
    }
    (Some(name.to_string()), rest.trim_start().to_string())
}

/// True when `text` holds at least one well-formed `[markup]` tag.
///
/// Recognises open, close and self-closing tags with an optional attribute
/// tail. `\[` and `\]` escapes and `{$...}` interpolations are skipped, so
/// `\[[literal]]` reports no markup.
pub(crate) fn has_markup(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if matches!(bytes.get(index + 1), Some(b'[') | Some(b']') | Some(b'\\')) => {
                index += 2;
            }
            b'{' if bytes.get(index + 1) == Some(&b'$') => {
                index = skip_interpolation(bytes, index);
            }
            b'[' => {
                let Some(close) = markup_close(bytes, index) else {
                    index += 1;
                    continue;
                };
                if markup_tag_valid(&text[index + 1..close]) {
                    return true;
                }
                index = close + 1;
            }
            _ => index += 1,
        }
    }
    false
}

/// Offset of the `]` closing the tag opened at `open`, or `None` when the tag
/// is unterminated or nests another `[`.
fn markup_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b']' => return Some(index),
            b'[' | b'\n' => return None,
            b'\\' => index += 2,
            _ => index += 1,
        }
    }
    None
}

fn markup_tag_valid(inner: &str) -> bool {
    let body = inner.strip_prefix('/').unwrap_or(inner);
    let body = body.strip_suffix('/').unwrap_or(body);
    if body.is_empty() || body.contains('[') || body.contains(']') {
        return false;
    }
    let mut chars = body.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let name_end = body
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(body.len());
    body[..name_end]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A `<<name args>>` frame located inside a line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandFrame {
    pub name: String,
    pub args: String,
    /// Byte offset of `<<` within the scanned content.
    pub start: usize,
    /// Byte offset just past `>>`.
    pub end: usize,
}

/// Finds the first `<<...>>` frame at or after `from`, skipping quoted text.
/// Returns `None` when an opener has no matching `>>`.
pub(crate) fn find_command(content: &str, from: usize) -> Option<CommandFrame> {
    let bytes = content.as_bytes();
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'"' {
            index = skip_string(bytes, index);
            continue;
        }
        if bytes[index] == b'<' && bytes[index + 1] == b'<' {
            let inner_start = index + 2;
            let close = find_close(bytes, inner_start)?;
            let inner = &content[inner_start..close];
            let (name, args) = split_command_inner(inner);
            return Some(CommandFrame {
                name,
                args,
                start: index,
                end: close + 2,
            });
        }
        index += 1;
    }
    None
}

fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == b'"' {
            index = skip_string(bytes, index);
            continue;
        }
        if bytes[index] == b'>' && bytes[index + 1] == b'>' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn split_command_inner(inner: &str) -> (String, String) {
    let trimmed = inner.trim();
    let cut = trimmed
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(trimmed.len());
    (
        trimmed[..cut].to_string(),
        trimmed[cut..].trim().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_drops_only_the_phantom_final_line() {
        let lines = split_lines("one\r\ntwo\n");
        assert_eq!(
            lines
                .iter()
                .map(|line| (line.number, line.text))
                .collect::<Vec<_>>(),
            [(1, "one"), (2, "two")]
        );
        // No trailing newline: the last line is real, not a phantom.
        assert_eq!(split_lines("one\ntwo").len(), 2);
        assert!(split_lines("").is_empty() || split_lines("")[0].text.is_empty());
    }

    #[test]
    fn indentation_counts_a_tab_as_four_columns() {
        let (prefix, rest) = split_indent("\t  -> option");
        assert_eq!(prefix, "\t  ");
        assert_eq!(rest, "-> option");
        assert_eq!(indent_width(prefix), TAB_WIDTH + 2);
        assert_eq!(indent_width("      "), 6);
    }

    #[test]
    fn char_col_counts_characters_not_bytes() {
        let line = "Ada: café.";
        let offset = line.find('c').expect("present");
        assert_eq!(char_col(line, offset), 6);
        let offset = line.find('f').expect("present");
        // `é` is two bytes but one column, so `f` sits at character 8.
        assert_eq!(char_col(line, offset), 8);
        let offset = line.find('.').expect("present");
        assert_eq!(char_col(line, offset), 10);
        assert_eq!(
            char_col(line, line.len() + 10),
            line.chars().count() as u32 + 1
        );
    }

    #[test]
    fn scan_line_separates_text_line_id_and_tags() {
        let scan = scan_line("Ada: Hello there. #line:abc123 #warm #night");
        assert_eq!(scan.text, "Ada: Hello there.");
        assert_eq!(scan.line_id.as_deref(), Some("abc123"));
        assert_eq!(scan.tags, ["warm", "night"]);
        assert!(scan.issues.is_empty());
    }

    #[test]
    fn scan_line_keeps_a_bare_hash_as_prose() {
        let scan = scan_line("Press # to continue. #line:k1");
        assert_eq!(scan.text, "Press # to continue.");
        assert_eq!(scan.line_id.as_deref(), Some("k1"));
        assert!(scan.issues.is_empty());
    }

    #[test]
    fn scan_line_unescapes_a_backslashed_hash() {
        let scan = scan_line(r"A literal \# sign. #line:e1");
        assert_eq!(scan.text, "A literal # sign.");
        assert_eq!(scan.line_id.as_deref(), Some("e1"));
    }

    #[test]
    fn scan_line_ignores_boundaries_inside_quotes_urls_and_interpolation() {
        let scan = scan_line("See http://example.com/a#b now. #line:u1");
        assert_eq!(scan.text, "See http://example.com/a#b now.");
        assert_eq!(scan.line_id.as_deref(), Some("u1"));

        let scan = scan_line("Quoted \"# not a tag\" here. #line:q1");
        assert_eq!(scan.text, "Quoted \"# not a tag\" here.");
        assert!(scan.tags.is_empty());

        let scan = scan_line("Holds {$value} intact. #line:i1");
        assert_eq!(scan.text, "Holds {$value} intact.");
    }

    #[test]
    fn scan_line_stops_at_a_comment_before_or_inside_the_tag_region() {
        let scan = scan_line("Text here. // dropped");
        assert_eq!(scan.text, "Text here.");
        assert!(scan.line_id.is_none());

        let scan = scan_line("Text here. #line:c1 #kept // dropped");
        assert_eq!(scan.text, "Text here.");
        assert_eq!(scan.line_id.as_deref(), Some("c1"));
        assert_eq!(scan.tags, ["kept"]);
    }

    #[test]
    fn scan_line_reports_bad_tag_regions() {
        let scan = scan_line("Text. #line:m1 #ok not-a-tag");
        assert_eq!(
            scan.issues
                .iter()
                .map(|(_, issue)| *issue)
                .collect::<Vec<_>>(),
            [ScanIssue::MalformedHashtag]
        );

        let scan = scan_line("Text. #line:");
        assert_eq!(scan.issues[0].1, ScanIssue::EmptyLineId);
        assert!(scan.line_id.is_none());

        let scan = scan_line("Text. #line:d1 #line:d1");
        assert_eq!(scan.issues[0].1, ScanIssue::DuplicateLineId);
        assert_eq!(scan.line_id.as_deref(), Some("d1"));
    }

    #[test]
    fn scan_line_reports_issue_offsets_that_map_to_characters() {
        let content = "Café text. #line:x1 #ok broken";
        let scan = scan_line(content);
        let (offset, _) = scan.issues[0];
        assert_eq!(&content[offset..offset + "broken".len()], "broken");
        assert_eq!(
            char_col(content, offset) as usize,
            content[..offset].chars().count() + 1
        );
    }

    #[test]
    fn split_speaker_accepts_names_and_rejects_lookalikes() {
        assert_eq!(
            split_speaker("Ada: Hello."),
            (Some("Ada".to_string()), "Hello.".to_string())
        );
        assert_eq!(
            split_speaker("Mary-Jane: Hi."),
            (Some("Mary-Jane".to_string()), "Hi.".to_string())
        );
        assert_eq!(
            split_speaker("_Narrator: Hi."),
            (Some("_Narrator".to_string()), "Hi.".to_string())
        );
        // A leading digit is a timestamp, not a character.
        assert_eq!(split_speaker("10:30 in the morning.").0, None);
        // A URL scheme is not a character.
        assert_eq!(split_speaker("http://example.com").0, None);
        // Whitespace is not allowed in a character name.
        assert_eq!(split_speaker("Old Man: Hello.").0, None);
        assert_eq!(split_speaker("No colon at all").0, None);
        assert_eq!(split_speaker(": nameless").0, None);
        // A command line is never a speaker line.
        assert_eq!(split_speaker("<<if $x>>").0, None);
    }

    #[test]
    fn has_markup_recognises_only_well_formed_tags() {
        assert!(has_markup("[wave]hello[/wave]"));
        assert!(has_markup("[bold color=\"red\"]hi[/bold]"));
        assert!(has_markup("leading text [em]mid[/em] trailing"));
        assert!(!has_markup("no brackets at all"));
        assert!(!has_markup("a bare [ bracket"));
        assert!(!has_markup("an unclosed [wave tag"));
        assert!(!has_markup(r"escaped \[wave\] text"));
        assert!(!has_markup("[1notaname]"));
        assert!(!has_markup("interpolation {$value} only"));
        assert!(!has_markup("[multi\nline]"));
    }

    #[test]
    fn find_command_locates_frames_and_skips_quotes() {
        let frame = find_command("<<jump Target>>", 0).expect("a frame");
        assert_eq!(frame.name, "jump");
        assert_eq!(frame.args, "Target");
        assert_eq!(frame.start, 0);
        assert_eq!(frame.end, "<<jump Target>>".len());

        let frame = find_command("Ada: Buy it. <<if $gold >= 10>>", 0).expect("a frame");
        assert_eq!(frame.name, "if");
        assert_eq!(frame.args, "$gold >= 10");

        // `<<` inside a quoted string is dialogue, not a command.
        assert_eq!(
            find_command("She said \"<<not a command>>\" loudly.", 0),
            None
        );
        // An opener with no closer is reported as absent, not as a frame.
        assert_eq!(find_command("<<unterminated", 0), None);
        // Searching resumes from `from`.
        assert_eq!(
            find_command("<<a>> then <<b>>", 6)
                .map(|frame| frame.name)
                .as_deref(),
            Some("b")
        );
    }
}
