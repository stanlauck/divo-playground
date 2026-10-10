// SPDX-License-Identifier: MIT OR Apache-2.0

//! Page geometry, layout profiles and word wrapping.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Supported standard page sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageSize {
    /// US Letter, 8.5 x 11 in.
    Letter,
    /// ISO A4, 210 x 297 mm (8.27 x 11.69 in).
    A4,
}

impl PageSize {
    /// Physical geometry the default profile is derived from.
    pub const fn geometry(self) -> PageGeometry {
        match self {
            Self::Letter => PageGeometry {
                width_in: 8.5,
                height_in: 11.0,
                margin_top_in: 1.0,
                margin_bottom_in: 1.0,
                margin_left_in: 1.5,
                margin_right_in: 1.0,
                chars_per_inch: 10,
                lines_per_inch: 6,
            },
            Self::A4 => PageGeometry {
                width_in: 8.27,
                height_in: 11.69,
                margin_top_in: 1.0,
                margin_bottom_in: 1.0,
                margin_left_in: 1.5,
                margin_right_in: 1.0,
                chars_per_inch: 10,
                lines_per_inch: 6,
            },
        }
    }

    /// The default layout profile for this page size.
    pub fn profile(self) -> LayoutProfile {
        match self {
            Self::Letter => LayoutProfile {
                page_size: "letter".to_string(),
                lines_per_page: 55,
                ..LayoutProfile::base()
            },
            Self::A4 => LayoutProfile {
                page_size: "a4".to_string(),
                lines_per_page: 58,
                ..LayoutProfile::base()
            },
        }
    }
}

/// Physical page constants (inches, Courier 12 pt density). Informational:
/// the estimator itself only uses the integer values in [`LayoutProfile`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    pub width_in: f64,
    pub height_in: f64,
    pub margin_top_in: f64,
    pub margin_bottom_in: f64,
    pub margin_left_in: f64,
    pub margin_right_in: f64,
    /// Courier 12 pt: 10 characters per inch.
    pub chars_per_inch: u32,
    /// Courier 12 pt: 6 lines per inch.
    pub lines_per_inch: u32,
}

/// Every number the estimator uses. All widths are in characters, all
/// heights in lines. Overridable through `--profile profile.json`; missing
/// fields take the Letter defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutProfile {
    /// Label echoed in the output (`letter`, `a4` or anything custom).
    pub page_size: String,
    /// Usable text lines per page.
    pub lines_per_page: u32,
    /// Wrap width of action paragraphs.
    pub action_width: u32,
    /// Wrap width of scene headings.
    pub scene_heading_width: u32,
    /// Wrap width of character cues.
    pub character_width: u32,
    /// Wrap width of parentheticals.
    pub parenthetical_width: u32,
    /// Wrap width of dialogue text.
    pub dialogue_width: u32,
    /// Wrap width of each dual-dialogue column (cues, parentheticals and
    /// text alike).
    pub dual_dialogue_width: u32,
    /// Wrap width of transitions.
    pub transition_width: u32,
    /// Wrap width of centered text.
    pub centered_width: u32,
    /// A scene heading must be followed by at least this many usable lines
    /// on the same page, otherwise it moves to the next page.
    pub heading_keep_lines: u32,
    /// When a dialogue block splits across pages, at least this many body
    /// lines stay with the cue on the current page.
    pub dialogue_split_min_lines: u32,
}

impl LayoutProfile {
    fn base() -> Self {
        Self {
            page_size: "letter".to_string(),
            lines_per_page: 55,
            action_width: 60,
            scene_heading_width: 60,
            character_width: 33,
            parenthetical_width: 25,
            dialogue_width: 35,
            dual_dialogue_width: 28,
            transition_width: 60,
            centered_width: 60,
            heading_keep_lines: 2,
            dialogue_split_min_lines: 2,
        }
    }

    /// Parses a profile from JSON. Missing fields take the Letter defaults.
    pub fn from_json(text: &str) -> Result<Self> {
        let profile: Self =
            serde_json::from_str(text).map_err(|e| Error::InvalidProfile(e.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Checks that every number is in its documented range.
    pub fn validate(&self) -> Result<()> {
        if self.page_size.is_empty() || self.page_size.chars().count() > 64 {
            return Err(Error::InvalidProfile(
                "page_size must be 1..=64 characters".into(),
            ));
        }
        if !(8..=1000).contains(&self.lines_per_page) {
            return Err(Error::InvalidProfile(
                "lines_per_page must be in 8..=1000".into(),
            ));
        }
        for (name, width) in [
            ("action_width", self.action_width),
            ("scene_heading_width", self.scene_heading_width),
            ("character_width", self.character_width),
            ("parenthetical_width", self.parenthetical_width),
            ("dialogue_width", self.dialogue_width),
            ("dual_dialogue_width", self.dual_dialogue_width),
            ("transition_width", self.transition_width),
            ("centered_width", self.centered_width),
        ] {
            if !(1..=1000).contains(&width) {
                return Err(Error::InvalidProfile(format!("{name} must be in 1..=1000")));
            }
        }
        if self.heading_keep_lines + 2 > self.lines_per_page {
            return Err(Error::InvalidProfile(
                "heading_keep_lines must be at most lines_per_page - 2".into(),
            ));
        }
        if self.dialogue_split_min_lines < 1
            || self.dialogue_split_min_lines + 3 > self.lines_per_page
        {
            return Err(Error::InvalidProfile(
                "dialogue_split_min_lines must be in 1..=lines_per_page - 3".into(),
            ));
        }
        Ok(())
    }
}

impl Default for LayoutProfile {
    fn default() -> Self {
        PageSize::Letter.profile()
    }
}

/// Greedy word wrap on single spaces. Each Unicode scalar value is one
/// column. Leading whitespace of the input is kept on the first line; runs
/// of inner spaces collapse to one. A word longer than `width` is split at
/// `width` columns. An empty line yields exactly one empty output line.
pub fn wrap(text: &str, width: u32) -> Vec<String> {
    let width = width.max(1) as usize;
    let body = text.trim_end();
    let indent_len = body.len() - body.trim_start().len();
    let indent = &body[..indent_len];
    let indent_cols = indent.chars().count();
    let words: Vec<&str> = body[indent_len..]
        .split(' ')
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return vec![indent.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::from(indent);
    let mut current_cols = indent_cols.min(width);
    let mut current_has_word = false;
    for word in words {
        let word_cols = word.chars().count();
        if !current_has_word {
            push_word(
                &mut lines,
                &mut current,
                &mut current_cols,
                word,
                word_cols,
                width,
            );
            current_has_word = true;
            continue;
        }
        if current_cols + 1 + word_cols <= width {
            current.push(' ');
            current.push_str(word);
            current_cols += 1 + word_cols;
        } else {
            lines.push(std::mem::take(&mut current));
            current_cols = 0;
            push_word(
                &mut lines,
                &mut current,
                &mut current_cols,
                word,
                word_cols,
                width,
            );
        }
    }
    lines.push(current);
    lines
}

/// Appends a word to `current`, hard-splitting it when it is wider than the
/// remaining columns of a fresh line.
fn push_word(
    lines: &mut Vec<String>,
    current: &mut String,
    current_cols: &mut usize,
    word: &str,
    word_cols: usize,
    width: usize,
) {
    if *current_cols + word_cols <= width {
        current.push_str(word);
        *current_cols += word_cols;
        return;
    }
    let mut chars = word.chars();
    loop {
        let room = width - *current_cols;
        let chunk: String = chars.by_ref().take(room).collect();
        if chunk.is_empty() {
            break;
        }
        let chunk_cols = chunk.chars().count();
        current.push_str(&chunk);
        *current_cols += chunk_cols;
        if chunk_cols < room {
            break;
        }
        // The line is full; is there more of the word?
        let mut peek = chars.clone();
        if peek.next().is_none() {
            break;
        }
        lines.push(std::mem::take(current));
        *current_cols = 0;
    }
}

/// Wraps several source lines, concatenating their wrapped output.
pub fn wrap_all(lines: &[String], width: u32) -> Vec<String> {
    lines.iter().flat_map(|l| wrap(l, width)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_exact_width_fits_on_one_line() {
        let text = "a".repeat(30) + " " + &"b".repeat(29);
        assert_eq!(text.chars().count(), 60);
        assert_eq!(wrap(&text, 60).len(), 1);
        let text = "a".repeat(30) + " " + &"b".repeat(30);
        assert_eq!(wrap(&text, 60).len(), 2);
    }

    #[test]
    fn wrap_is_greedy_on_spaces() {
        assert_eq!(
            wrap("one two three four", 9),
            vec!["one two", "three", "four"]
        );
    }

    #[test]
    fn wrap_hard_splits_long_words() {
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("ab cdefghijk", 4), vec!["ab", "cdef", "ghij", "k"]);
        assert_eq!(wrap("abcdefgh", 4), vec!["abcd", "efgh"]);
    }

    #[test]
    fn wrap_keeps_indent_and_collapses_inner_spaces() {
        assert_eq!(wrap("    x   y", 10), vec!["    x y"]);
        assert_eq!(wrap("", 10), vec![""]);
        assert_eq!(wrap("   ", 10), vec![""]);
    }

    #[test]
    fn wrap_counts_scalar_values_not_bytes() {
        let text = "ééééé ààààà";
        assert_eq!(wrap(text, 11).len(), 1);
        assert_eq!(wrap(text, 10).len(), 2);
    }

    #[test]
    fn default_profiles_match_documented_numbers() {
        let letter = PageSize::Letter.profile();
        assert_eq!(letter.lines_per_page, 55);
        assert_eq!(letter.action_width, 60);
        assert_eq!(letter.dialogue_width, 35);
        assert_eq!(letter.parenthetical_width, 25);
        assert_eq!(letter.character_width, 33);
        assert_eq!(letter.dual_dialogue_width, 28);
        let a4 = PageSize::A4.profile();
        assert_eq!(a4.lines_per_page, 58);
        assert_eq!(a4.action_width, letter.action_width);
        assert!(letter.validate().is_ok());
        assert!(a4.validate().is_ok());
    }

    #[test]
    fn profile_json_overrides_only_given_fields() {
        let p = LayoutProfile::from_json(r#"{"lines_per_page": 54}"#).unwrap();
        assert_eq!(p.lines_per_page, 54);
        assert_eq!(p.action_width, 60);
        assert_eq!(p.page_size, "letter");
    }

    #[test]
    fn profile_validation_rejects_bad_numbers() {
        assert!(LayoutProfile::from_json(r#"{"lines_per_page": 4}"#).is_err());
        assert!(LayoutProfile::from_json(r#"{"dialogue_width": 0}"#).is_err());
        assert!(LayoutProfile::from_json(r#"{"heading_keep_lines": 60}"#).is_err());
        assert!(LayoutProfile::from_json(r#"{"dialogue_split_min_lines": 0}"#).is_err());
        assert!(LayoutProfile::from_json("not json").is_err());
        assert!(LayoutProfile::from_json(r#"{"page_size": ""}"#).is_err());
    }
}
