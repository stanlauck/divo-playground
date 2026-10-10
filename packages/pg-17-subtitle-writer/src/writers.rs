// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Cue, Document, Error, Result};
use std::fmt::Write as _;

/// Output format selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// SubRip (`.srt`).
    Srt,
    /// WebVTT (`.vtt`).
    Vtt,
    /// TTML 1.0 / IMSC-style (`.ttml`).
    Ttml,
}

impl Format {
    /// Parses `srt`, `vtt` (or `webvtt`) and `ttml`, ASCII case-insensitively.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "vtt" | "webvtt" => Some(Self::Vtt),
            "ttml" => Some(Self::Ttml),
            _ => None,
        }
    }
    /// Conventional file extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ttml => "ttml",
        }
    }
}

/// Writes the document in the chosen format.
pub fn write(document: &Document, format: Format) -> Result<String> {
    match format {
        Format::Srt => write_srt(document),
        Format::Vtt => write_vtt(document),
        Format::Ttml => write_ttml(document),
    }
}

/// Formats milliseconds as `HH:MM:SS` + `separator` + `mmm`.
/// Hours are two digits; the model caps times below 100 hours.
pub fn format_timecode(ms: u64, separator: char) -> String {
    let hours = ms / 3_600_000;
    let minutes = ms / 60_000 % 60;
    let seconds = ms / 1000 % 60;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}{separator}{millis:03}")
}

/// Rejects cues that no subtitle format can carry: an empty (or
/// whitespace-only) line would end the cue block in SRT/WebVTT, and a cue
/// whose end is not after its start has no presentation interval.
fn writable(document: &Document) -> Result<()> {
    document.validate()?;
    for (i, cue) in document.cues.iter().enumerate() {
        if cue.end_ms <= cue.start_ms {
            return Err(Error::invalid(
                format!("cues[{i}].end_ms"),
                "end must be after start to write a cue",
            ));
        }
        for (j, line) in cue.lines.iter().enumerate() {
            if line.trim().is_empty() {
                return Err(Error::invalid(
                    format!("cues[{i}].lines[{j}]"),
                    "empty line cannot be written",
                ));
            }
        }
    }
    Ok(())
}

/// SubRip: sequential 1-based numbering, `HH:MM:SS,mmm`, speaker as a
/// `NAME: ` prefix on the first line, no escaping, blocks separated by one
/// empty line, LF line endings, no BOM.
pub fn write_srt(document: &Document) -> Result<String> {
    writable(document)?;
    let mut out = String::new();
    for (i, cue) in document.cues.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "{}", i + 1);
        let _ = writeln!(
            out,
            "{} --> {}",
            format_timecode(cue.start_ms, ','),
            format_timecode(cue.end_ms, ',')
        );
        for (j, line) in cue.lines.iter().enumerate() {
            if j == 0 {
                if let Some(speaker) = &cue.speaker {
                    out.push_str(speaker);
                    out.push_str(": ");
                }
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Escapes `&`, `<` and `>` (WebVTT cue text and XML character data).
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes `&`, `<`, `>` and `"` (XML attribute values).
fn escape_attribute(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}

/// WebVTT: `WEBVTT` header, cue identifier line = cue id, `HH:MM:SS.mmm`,
/// speaker as a `<v NAME>` voice span opened on the first line (the closing
/// tag is optional at the end of a cue and is omitted), `&`/`<`/`>` escaped
/// in the payload and in the speaker name, LF line endings, no BOM.
pub fn write_vtt(document: &Document) -> Result<String> {
    writable(document)?;
    let mut out = String::from("WEBVTT\n");
    for cue in &document.cues {
        out.push('\n');
        let _ = writeln!(out, "{}", cue.id);
        let _ = writeln!(
            out,
            "{} --> {}",
            format_timecode(cue.start_ms, '.'),
            format_timecode(cue.end_ms, '.')
        );
        for (j, line) in cue.lines.iter().enumerate() {
            if j == 0 {
                if let Some(speaker) = &cue.speaker {
                    let _ = write!(out, "<v {}>", escape_text(speaker));
                }
            }
            out.push_str(&escape_text(line));
            out.push('\n');
        }
    }
    Ok(out)
}

/// `true` when `id` is usable as an XML `xml:id` (ASCII NCName subset).
fn is_ncname(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// TTML 1.0 (IMSC-style profile subset): one `<p>` per cue with `begin`/`end`
/// clock times (`ttp:timeBase="media"`), `<br/>` between lines, speakers as
/// `ttm:agent` elements in the head referenced by `ttm:agent` on each `<p>`.
///
/// `xml:id` of every `<p>` is the cue id when all ids are unique ASCII
/// NCNames (`[A-Za-z_][A-Za-z0-9_.-]*`), otherwise `c1`, `c2`, ... in order.
/// `&`, `<`, `>` are escaped in text and `"` additionally in attributes.
pub fn write_ttml(document: &Document) -> Result<String> {
    writable(document)?;
    let mut agents: Vec<&str> = Vec::new();
    for cue in &document.cues {
        if let Some(speaker) = cue.speaker.as_deref() {
            if !agents.contains(&speaker) {
                agents.push(speaker);
            }
        }
    }
    let agent_index = |cue: &Cue| -> Option<usize> {
        cue.speaker
            .as_deref()
            .and_then(|s| agents.iter().position(|a| *a == s))
    };
    let own_ids = {
        let mut ids: Vec<&str> = document.cues.iter().map(|c| c.id.as_str()).collect();
        let all_ncnames = ids.iter().all(|id| is_ncname(id));
        ids.sort_unstable();
        all_ncnames && ids.windows(2).all(|w| w[0] != w[1])
    };
    let language = document.language.as_deref().unwrap_or("und");

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:ttm=\"http://www.w3.org/ns/ttml#metadata\" xmlns:ttp=\"http://www.w3.org/ns/ttml#parameter\" xml:lang=\"{}\" ttp:timeBase=\"media\">",
        escape_attribute(language)
    );
    out.push_str("  <head>\n    <metadata>\n");
    if let Some(title) = &document.title {
        let _ = writeln!(out, "      <ttm:title>{}</ttm:title>", escape_text(title));
    }
    for (i, agent) in agents.iter().enumerate() {
        let _ = writeln!(
            out,
            "      <ttm:agent type=\"person\" xml:id=\"agent{}\"><ttm:name type=\"full\">{}</ttm:name></ttm:agent>",
            i + 1,
            escape_text(agent)
        );
    }
    out.push_str("    </metadata>\n  </head>\n  <body>\n    <div>\n");
    for (i, cue) in document.cues.iter().enumerate() {
        let _ = write!(
            out,
            "      <p xml:id=\"{}\" begin=\"{}\" end=\"{}\"",
            if own_ids {
                cue.id.clone()
            } else {
                format!("c{}", i + 1)
            },
            format_timecode(cue.start_ms, '.'),
            format_timecode(cue.end_ms, '.')
        );
        if let Some(index) = agent_index(cue) {
            let _ = write!(out, " ttm:agent=\"agent{}\"", index + 1);
        }
        out.push('>');
        for (j, line) in cue.lines.iter().enumerate() {
            if j > 0 {
                out.push_str("<br/>");
            }
            out.push_str(&escape_text(line));
        }
        out.push_str("</p>\n");
    }
    out.push_str("    </div>\n  </body>\n</tt>\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ncname_subset() {
        assert!(is_ncname("a"));
        assert!(is_ncname("_x1.y-z"));
        assert!(!is_ncname(""));
        assert!(!is_ncname("1a"));
        assert!(!is_ncname("-a"));
        assert!(!is_ncname("a b"));
        assert!(!is_ncname("a:b"));
        assert!(!is_ncname("ид"));
    }

    #[test]
    fn attribute_escaping_adds_quotes() {
        assert_eq!(escape_attribute("a\"<&>"), "a&quot;&lt;&amp;&gt;");
    }
}
