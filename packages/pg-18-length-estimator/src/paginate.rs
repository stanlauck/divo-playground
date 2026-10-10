// SPDX-License-Identifier: MIT OR Apache-2.0

//! Places parsed elements on pages and measures scenes.

use crate::layout::{wrap, wrap_all, LayoutProfile};
use crate::parser::{DialogueBlock, DialogueLine, Document, Element};
use serde::{Deserialize, Serialize};

/// Output schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Length of one scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneLength {
    /// Zero-based position in the script.
    pub index: usize,
    /// Scene number from a `#number#` suffix, if any.
    pub number: Option<String>,
    /// Heading text; `null` for material before the first heading.
    pub heading: Option<String>,
    /// First page of the scene (1-based; the title page is page 1).
    pub start_page: u32,
    /// Page holding the last line owned by the scene.
    pub end_page: u32,
    /// Lines from the heading through the line before the next heading.
    pub line_count: u32,
    /// `round_half_up(line_count * 8 / lines_per_page)`, at least 1 when
    /// `line_count > 0`.
    pub eighths: u32,
    /// `eighths` as a `"1 2/8"`-style string.
    pub pages: String,
}

/// Totals over the whole script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    /// Pages used, including the title page.
    pub pages: u32,
    /// Sum of the scene eighths.
    pub eighths: u32,
    /// Body lines used (all scene `line_count`s summed).
    pub lines: u32,
}

/// The complete estimate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estimate {
    pub version: u32,
    /// Label of the page size (`letter`, `a4` or the profile's label).
    pub page_size: String,
    /// Every number the estimate was computed with.
    pub layout: LayoutProfile,
    /// Whether a title page was found (it occupies page 1).
    pub title_page: bool,
    pub scenes: Vec<SceneLength>,
    pub total: Totals,
}

/// Formats eighths as `"0"`, `"3/8"`, `"1"` or `"1 2/8"`.
pub fn eighths_to_pages(eighths: u32) -> String {
    let whole = eighths / 8;
    let rest = eighths % 8;
    match (whole, rest) {
        (0, 0) => "0".to_string(),
        (0, r) => format!("{r}/8"),
        (w, 0) => w.to_string(),
        (w, r) => format!("{w} {r}/8"),
    }
}

/// `round_half_up(lines * 8 / lines_per_page)`, at least 1 for non-empty.
pub fn lines_to_eighths(lines: u32, lines_per_page: u32) -> u32 {
    if lines == 0 {
        return 0;
    }
    let lpp = u64::from(lines_per_page.max(1));
    let n = u64::from(lines) * 8;
    let rounded = (2 * n + lpp) / (2 * lpp);
    rounded.max(1).min(u64::from(u32::MAX)) as u32
}

struct OpenScene {
    heading: Option<String>,
    number: Option<String>,
    start_abs: u64,
    start_page: u32,
}

struct Paginator<'a> {
    profile: &'a LayoutProfile,
    lpp: u32,
    /// Current page, 1-based; the body starts on page 2 after a title page.
    page: u32,
    /// Next free line on the current page, `0..=lpp`.
    line: u32,
    body_first_page: u32,
    open: Option<OpenScene>,
    scenes: Vec<SceneLength>,
}

impl<'a> Paginator<'a> {
    fn new(profile: &'a LayoutProfile, title_page: bool) -> Self {
        let first = if title_page { 2 } else { 1 };
        Self {
            profile,
            lpp: profile.lines_per_page,
            page: first,
            line: 0,
            body_first_page: first,
            open: None,
            scenes: Vec::new(),
        }
    }

    /// Absolute body line index of the cursor (0 at the top of the body).
    fn abs(&self) -> u64 {
        u64::from(self.page - self.body_first_page) * u64::from(self.lpp) + u64::from(self.line)
    }

    fn available(&self) -> u32 {
        self.lpp - self.line
    }

    fn new_page(&mut self) {
        self.page = self.page.saturating_add(1);
        self.line = 0;
    }

    /// One blank line before a block, unless the block starts a page.
    fn separator(&mut self) {
        if self.line >= self.lpp {
            self.new_page();
        } else if self.line != 0 {
            self.line += 1;
        }
    }

    /// Places `n` lines sequentially, spilling across pages.
    fn place_lines(&mut self, mut n: u32) {
        while n > 0 {
            if self.line >= self.lpp {
                self.new_page();
            }
            let take = n.min(self.available());
            self.line += take;
            n -= take;
        }
    }

    fn open_pseudo_scene_if_needed(&mut self) {
        if self.open.is_none() {
            self.open = Some(OpenScene {
                heading: None,
                number: None,
                start_abs: self.abs(),
                start_page: self.page,
            });
        }
    }

    fn close_scene(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        let end_abs = self.abs();
        let line_count = end_abs
            .saturating_sub(open.start_abs)
            .min(u64::from(u32::MAX)) as u32;
        let end_page = if line_count == 0 {
            open.start_page
        } else {
            let last = end_abs - 1;
            self.body_first_page + (last / u64::from(self.lpp)).min(u64::from(u32::MAX)) as u32
        };
        let eighths = lines_to_eighths(line_count, self.lpp);
        self.scenes.push(SceneLength {
            index: self.scenes.len(),
            number: open.number,
            heading: open.heading,
            start_page: open.start_page,
            end_page,
            line_count,
            eighths,
            pages: eighths_to_pages(eighths),
        });
    }

    fn scene_heading(&mut self, text: &str, number: Option<&str>) {
        self.separator();
        let lines = wrap(text, self.profile.scene_heading_width).len() as u32;
        let needed = lines + self.profile.heading_keep_lines;
        if self.line != 0 && needed > self.available() {
            self.new_page();
        }
        self.close_scene();
        self.open = Some(OpenScene {
            heading: Some(text.to_string()),
            number: number.map(str::to_string),
            start_abs: self.abs(),
            start_page: self.page,
        });
        self.place_lines(lines);
    }

    fn paragraph(&mut self, lines: &[String], width: u32) {
        self.open_pseudo_scene_if_needed();
        self.separator();
        let n = wrap_all(lines, width).len() as u32;
        self.place_lines(n);
    }

    fn page_break(&mut self) {
        if self.line != 0 {
            self.open_pseudo_scene_if_needed();
            self.new_page();
        }
    }

    fn dialogue_body_lines(&self, block: &DialogueBlock, dual: bool) -> u32 {
        let (p_w, d_w) = if dual {
            (
                self.profile.dual_dialogue_width,
                self.profile.dual_dialogue_width,
            )
        } else {
            (
                self.profile.parenthetical_width,
                self.profile.dialogue_width,
            )
        };
        block
            .lines
            .iter()
            .map(|l| match l {
                DialogueLine::Parenthetical(t) => wrap(t, p_w).len() as u32,
                DialogueLine::Text(t) | DialogueLine::Lyric(t) => wrap(t, d_w).len() as u32,
            })
            .sum()
    }

    fn cue_lines(&self, name: &str, width: u32) -> u32 {
        wrap(name, width).len() as u32
    }

    fn dialogue(&mut self, block: &DialogueBlock) {
        self.open_pseudo_scene_if_needed();
        self.separator();
        let cue_w = self.profile.character_width;
        let min_keep = self.profile.dialogue_split_min_lines;
        let mut cue = self.cue_lines(&block.character, cue_w);
        let mut body = self.dialogue_body_lines(block, false);
        let contd = if block.character.to_uppercase().ends_with("(CONT'D)") {
            block.character.clone()
        } else {
            format!("{} (CONT'D)", block.character)
        };
        let contd_lines = self.cue_lines(&contd, cue_w);
        loop {
            if self.line >= self.lpp {
                self.new_page();
            }
            let available = self.available();
            if cue + body <= available {
                self.place_lines(cue + body);
                return;
            }
            // Lines of dialogue that can stay here with a `(MORE)` line.
            let keep = available.saturating_sub(cue + 1);
            if keep >= min_keep && keep < body {
                self.place_lines(cue + keep + 1);
                self.new_page();
                body -= keep;
                cue = contd_lines;
                continue;
            }
            if self.line != 0 {
                self.new_page();
                continue;
            }
            // Degenerate: the block cannot be split by the rule even on an
            // empty page (huge cue or tiny page); place it sequentially.
            self.place_lines(cue + body);
            return;
        }
    }

    fn dual_dialogue(&mut self, left: &DialogueBlock, right: &DialogueBlock) {
        self.open_pseudo_scene_if_needed();
        self.separator();
        let w = self.profile.dual_dialogue_width;
        let l = self.cue_lines(&left.character, w) + self.dialogue_body_lines(left, true);
        let r = self.cue_lines(&right.character, w) + self.dialogue_body_lines(right, true);
        let height = l.max(r);
        if self.line != 0 && height > self.available() {
            self.new_page();
        }
        self.place_lines(height);
    }

    fn run(mut self, doc: &Document) -> Estimate {
        let elements = &doc.elements;
        let mut i = 0;
        while i < elements.len() {
            match &elements[i] {
                Element::SceneHeading { text, number } => {
                    self.scene_heading(text, number.as_deref());
                }
                Element::Action(lines) | Element::Lyrics(lines) => {
                    self.paragraph(lines, self.profile.action_width);
                }
                Element::Centered(lines) => {
                    self.paragraph(lines, self.profile.centered_width);
                }
                Element::Transition(text) => {
                    let lines = vec![text.clone()];
                    self.paragraph(&lines, self.profile.transition_width);
                }
                Element::PageBreak => self.page_break(),
                Element::Dialogue(block) => {
                    let partner = match elements.get(i + 1) {
                        Some(Element::Dialogue(next)) if next.dual && !block.dual => Some(next),
                        _ => None,
                    };
                    if let Some(right) = partner {
                        self.dual_dialogue(block, right);
                        i += 1;
                    } else {
                        self.dialogue(block);
                    }
                }
            }
            i += 1;
        }
        self.close_scene();
        let body_lines = self.abs();
        let lpp = u64::from(self.lpp);
        let body_pages = body_lines.div_ceil(lpp);
        let total_pages = body_pages + u64::from(doc.has_title_page());
        let eighths: u64 = self.scenes.iter().map(|s| u64::from(s.eighths)).sum();
        Estimate {
            version: SCHEMA_VERSION,
            page_size: self.profile.page_size.clone(),
            layout: self.profile.clone(),
            title_page: doc.has_title_page(),
            scenes: self.scenes,
            total: Totals {
                pages: total_pages.min(u64::from(u32::MAX)) as u32,
                eighths: eighths.min(u64::from(u32::MAX)) as u32,
                lines: body_lines.min(u64::from(u32::MAX)) as u32,
            },
        }
    }
}

/// Lays out a parsed document with the given profile.
pub fn paginate(doc: &Document, profile: &LayoutProfile) -> Estimate {
    Paginator::new(profile, doc.has_title_page()).run(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PageSize;
    use crate::parser::parse;

    fn letter(text: &str) -> Estimate {
        paginate(&parse(text).unwrap(), &PageSize::Letter.profile())
    }

    fn action_lines(n: usize) -> String {
        (0..n)
            .map(|i| format!("Line {i}."))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn eighths_rounding_boundaries() {
        // 55 lines per page: one eighth is 6.875 lines.
        assert_eq!(lines_to_eighths(0, 55), 0);
        assert_eq!(lines_to_eighths(1, 55), 1);
        assert_eq!(lines_to_eighths(3, 55), 1);
        assert_eq!(lines_to_eighths(10, 55), 1); // 1.4545
        assert_eq!(lines_to_eighths(11, 55), 2); // 1.6
        assert_eq!(lines_to_eighths(55, 55), 8);
        assert_eq!(lines_to_eighths(110, 55), 16);
        // Exact half rounds up: 4 lines on a 64-line page = 0.5 eighths.
        assert_eq!(lines_to_eighths(4, 64), 1);
        assert_eq!(lines_to_eighths(12, 64), 2); // 1.5 -> 2
        assert_eq!(lines_to_eighths(20, 64), 3); // 2.5 -> 3
        assert_eq!(lines_to_eighths(19, 64), 2); // 2.375 -> 2
    }

    #[test]
    fn pages_string_formats() {
        assert_eq!(eighths_to_pages(0), "0");
        assert_eq!(eighths_to_pages(3), "3/8");
        assert_eq!(eighths_to_pages(8), "1");
        assert_eq!(eighths_to_pages(10), "1 2/8");
        assert_eq!(eighths_to_pages(16), "2");
    }

    #[test]
    fn single_scene_counts_heading_blank_and_action() {
        let e = letter("INT. ROOM - DAY\n\nShe waits.\n");
        assert_eq!(e.scenes.len(), 1);
        let s = &e.scenes[0];
        assert_eq!(s.line_count, 3);
        assert_eq!(s.eighths, 1);
        assert_eq!((s.start_page, s.end_page), (1, 1));
        assert_eq!(e.total.pages, 1);
        assert_eq!(e.total.lines, 3);
        assert!(!e.title_page);
    }

    #[test]
    fn trailing_blank_before_next_heading_belongs_to_previous_scene() {
        let e = letter("INT. A - DAY\n\nOne.\n\nINT. B - DAY\n\nTwo.\n");
        assert_eq!(e.scenes[0].line_count, 4);
        assert_eq!(e.scenes[1].line_count, 3);
        assert_eq!(e.total.lines, 7);
    }

    #[test]
    fn cold_open_forms_pseudo_scene() {
        let e = letter("FADE IN:\n\nA street.\n\nINT. A - DAY\n\nOne.\n");
        assert_eq!(e.scenes.len(), 2);
        assert_eq!(e.scenes[0].heading, None);
        assert_eq!(e.scenes[0].line_count, 4);
        assert_eq!(e.scenes[1].heading.as_deref(), Some("INT. A - DAY"));
        assert_eq!(e.scenes[1].number, None);
    }

    #[test]
    fn scene_number_is_extracted() {
        let e = letter("INT. A - DAY #7#\n\nOne.\n");
        assert_eq!(e.scenes[0].number.as_deref(), Some("7"));
        assert_eq!(e.scenes[0].heading.as_deref(), Some("INT. A - DAY"));
    }

    #[test]
    fn title_page_occupies_page_one() {
        let e = letter("Title: X\n\nINT. A - DAY\n\nOne.\n");
        assert!(e.title_page);
        assert_eq!(e.scenes[0].start_page, 2);
        assert_eq!(e.total.pages, 2);
        assert_eq!(e.total.lines, 3);
    }

    #[test]
    fn title_page_alone_is_one_page() {
        let e = letter("Title: X\n");
        assert!(e.scenes.is_empty());
        assert_eq!(e.total.pages, 1);
        assert_eq!(e.total.lines, 0);
        assert_eq!(e.total.eighths, 0);
    }

    #[test]
    fn orphan_heading_moves_to_next_page() {
        // 52 action lines fill lines 0..52 of page 1; blank at 52; the
        // heading would land on line 53 (second-to-last) -> pushed.
        let text = format!("{}\n\nINT. B - DAY\n\nTwo.\n", action_lines(52));
        let e = letter(&text);
        assert_eq!(e.scenes[1].start_page, 2);
        assert_eq!(e.scenes[0].line_count, 55);
        assert_eq!(e.scenes[0].end_page, 1);
        // With one line less, the heading sits on line 52 and stays.
        let text = format!("{}\n\nINT. B - DAY\n\nTwo.\n", action_lines(51));
        let e = letter(&text);
        assert_eq!(e.scenes[1].start_page, 1);
        assert_eq!(e.scenes[1].end_page, 1);
        assert_eq!(e.total.lines, 55);
    }

    #[test]
    fn page_break_forces_new_page_and_counts_towards_scene() {
        let e = letter("INT. A - DAY\n\nOne.\n\n===\n\nINT. B - DAY\n\nTwo.\n");
        assert_eq!(e.scenes[0].line_count, 55);
        assert_eq!(e.scenes[0].eighths, 8);
        assert_eq!(e.scenes[1].start_page, 2);
        assert_eq!(e.total.pages, 2);
    }

    #[test]
    fn page_break_at_top_of_page_is_a_no_op() {
        let e = letter("===\n\nINT. A - DAY\n\nOne.\n");
        assert_eq!(e.scenes.len(), 1);
        assert_eq!(e.scenes[0].start_page, 1);
        assert_eq!(e.total.pages, 1);
    }

    #[test]
    fn dialogue_cue_never_ends_a_page() {
        // 53 action lines occupy 0..53, blank on 53, cue would be line 54.
        let text = format!("{}\n\nBOB\nHello.\n", action_lines(53));
        let e = letter(&text);
        // cue + 1 line moved to page 2: lines = 55 + 2.
        assert_eq!(e.total.lines, 57);
        assert_eq!(e.total.pages, 2);
    }

    #[test]
    fn long_dialogue_splits_with_more_and_contd() {
        // 48 action lines (0..48), blank 48, cue 49, 4 lines of dialogue
        // would reach 53 and fit. Use 6 lines: 49 cue + 6 = 55 > 55.
        let text = format!(
            "{}\n\nBOB\nOne.\nTwo.\nThree.\nFour.\nFive.\nSix.\n",
            action_lines(48)
        );
        let e = letter(&text);
        // Page 1: cue (49), keep 4 lines (50..53), (MORE) at 54 -> full.
        // Page 2: BOB (CONT'D) + 2 lines = 3 lines.
        assert_eq!(e.total.pages, 2);
        assert_eq!(e.total.lines, 58);
    }

    #[test]
    fn dialogue_too_short_to_split_moves_whole() {
        // cue at 52, lines 53, 54 -> fits exactly (3 lines).
        let text = format!("{}\n\nBOB\nOne.\nTwo.\n", action_lines(51));
        let e = letter(&text);
        assert_eq!(e.total.lines, 55);
        // Three lines of dialogue: keep would be 55-52-1-1 = 1 < 2 -> move.
        let text = format!("{}\n\nBOB\nOne.\nTwo.\nThree.\n", action_lines(51));
        let e = letter(&text);
        assert_eq!(e.total.lines, 59);
        assert_eq!(e.total.pages, 2);
    }

    #[test]
    fn very_long_dialogue_splits_repeatedly() {
        let speech = (0..200)
            .map(|i| format!("Word {i}."))
            .collect::<Vec<_>>()
            .join("\n");
        let e = letter(&format!("BOB\n{speech}\n"));
        assert!(e.total.pages >= 4);
        assert_eq!(e.scenes.len(), 1);
    }

    #[test]
    fn dual_dialogue_takes_max_of_both_columns() {
        let e = letter("BOB\nOne.\nTwo.\nThree.\n\nANNA ^\nUno.\n");
        // cue + 3 lines on the left = 4, right = 2 -> 4 lines.
        assert_eq!(e.total.lines, 4);
        // Not dual when the caret is on the first block.
        let e = letter("BOB ^\nOne.\n\nANNA\nUno.\n");
        assert_eq!(e.total.lines, 5);
    }

    #[test]
    fn dual_dialogue_wraps_at_column_width() {
        let long = "word ".repeat(12).trim_end().to_string(); // 59 chars
        let e = letter(&format!("BOB\n{long}\n\nANNA ^\nUno.\n"));
        // 59 chars at width 28 -> 3 lines; cue 1 -> 4.
        assert_eq!(e.total.lines, 4);
    }

    #[test]
    fn dual_dialogue_kept_together_across_page() {
        let text = format!("{}\n\nBOB\nOne.\nTwo.\n\nANNA ^\nUno.\n", action_lines(52));
        let e = letter(&text);
        assert_eq!(e.total.pages, 2);
        assert_eq!(e.total.lines, 58);
    }

    #[test]
    fn action_spills_across_pages_freely() {
        let e = letter(&action_lines(120));
        assert_eq!(e.total.pages, 3);
        assert_eq!(e.total.lines, 120);
        assert_eq!(e.scenes[0].end_page, 3);
    }

    #[test]
    fn wrapped_action_counts_wrapped_lines() {
        let long = "word ".repeat(30).trim_end().to_string(); // 149 chars
        let e = letter(&long);
        assert_eq!(e.total.lines, 3);
    }

    #[test]
    fn letter_and_a4_differ_in_lines_per_page() {
        let text = action_lines(57);
        let doc = parse(&text).unwrap();
        let l = paginate(&doc, &PageSize::Letter.profile());
        let a = paginate(&doc, &PageSize::A4.profile());
        assert_eq!(l.total.pages, 2);
        assert_eq!(a.total.pages, 1);
        assert_eq!(l.page_size, "letter");
        assert_eq!(a.page_size, "a4");
        assert_eq!(l.scenes[0].eighths, 8);
        assert_eq!(a.scenes[0].eighths, 8); // 57*8/58 = 7.86 -> 8
    }

    #[test]
    fn end_page_is_page_of_last_owned_line() {
        let text = format!(
            "INT. A - DAY\n\n{}\n\nINT. B - DAY\n\nX.\n",
            action_lines(60)
        );
        let e = letter(&text);
        assert_eq!(e.scenes[0].start_page, 1);
        assert_eq!(e.scenes[0].end_page, 2);
        assert_eq!(e.scenes[1].start_page, 2);
    }

    #[test]
    fn blocks_after_an_exactly_full_page_start_the_next_page_without_blank() {
        let full = action_lines(55);
        let e = letter(&format!("{full}\n\nINT. B - DAY\n\nX.\n"));
        assert_eq!(e.scenes[0].line_count, 55);
        assert_eq!(e.scenes[1].start_page, 2);
        assert_eq!(e.scenes[1].line_count, 3);
        let e = letter(&format!("{full}\n\nBOB\nOne.\n\nANNA ^\nUno.\n"));
        assert_eq!(e.total.lines, 57);
        let e = letter(&format!("{full}\n\nBOB\nOne.\n"));
        assert_eq!(e.total.lines, 57);
        let e = letter(&format!("{full}\n\nCUT TO:\n"));
        assert_eq!(e.total.lines, 56);
    }

    #[test]
    fn empty_document_has_no_scenes_and_no_pages() {
        let e = letter("");
        assert!(e.scenes.is_empty());
        assert_eq!(e.total.pages, 0);
        assert_eq!(e.total.lines, 0);
    }
}
