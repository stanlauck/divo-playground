// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub max_input_bytes: usize,
    pub max_objects: usize,
    pub max_pages: usize,
    pub max_decoded_page_bytes: usize,
    pub max_decoded_content_bytes: usize,
    pub max_operations: usize,
    pub max_form_depth: usize,
    pub max_glyphs: usize,
    pub max_text_bytes: usize,
    pub max_lines: usize,
    pub max_blocks: usize,
    pub max_doubts: usize,
    pub max_warnings: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 32 * 1024 * 1024,
            max_objects: 100_000,
            max_pages: 2_000,
            max_decoded_page_bytes: 16 * 1024 * 1024,
            max_decoded_content_bytes: 64 * 1024 * 1024,
            max_operations: 1_000_000,
            max_form_depth: 32,
            max_glyphs: 1_000_000,
            max_text_bytes: 16 * 1024 * 1024,
            max_lines: 100_000,
            max_blocks: 100_000,
            max_doubts: 100_000,
            max_warnings: 100_000,
        }
    }
}

impl ImportOptions {
    pub(crate) fn validate(&self) -> Result<()> {
        if [
            self.max_input_bytes,
            self.max_objects,
            self.max_pages,
            self.max_decoded_page_bytes,
            self.max_decoded_content_bytes,
            self.max_operations,
            self.max_form_depth,
            self.max_glyphs,
            self.max_text_bytes,
            self.max_lines,
            self.max_blocks,
            self.max_doubts,
            self.max_warnings,
        ]
        .contains(&0)
            || self.max_form_depth > 32
        {
            return Err(Error::InvalidOptions);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Bounds {
    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    SceneHeading,
    Action,
    Character,
    Dialogue,
    Parenthetical,
    Transition,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageStatus {
    Text,
    Blank,
    UnsupportedNoTextLayer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    AmbiguousUppercase,
    MissingSpeaker,
    UnexpectedParenthetical,
    PossibleHeaderFooter,
    MultipleColumns,
    UnsupportedTextOrientation,
    UnicodeMappingMissing,
    BackendUncertainText,
    ControlCharacters,
    UnrecognizedText,
    IncompleteSceneHeading,
    DialogueAcrossPage,
    UncertainCharacterCue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    NoExtractableText,
    BlankPage,
    ImageContentNotImported,
    BackendUnsupportedFont,
    BackendImageDecodeFailure,
    BackendInvalidResource,
    BackendUnknownOperator,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub kind: WarningKind,
    pub page: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub number: usize,
    pub width: f64,
    pub height: f64,
    pub rotation: u16,
    pub user_unit: f64,
    pub status: PageStatus,
    pub images: usize,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceLine {
    pub id: String,
    pub page: usize,
    pub index: usize,
    pub row: usize,
    pub draw_order: usize,
    /// Decoded text in geometric reading order, with missing word spaces inferred.
    pub text: String,
    /// Same ordered glyph strings before inferred spaces; not original PDF bytes.
    pub raw_text: String,
    /// Approximate font-em/advance bounds, not tight glyph outlines.
    pub bounds: Bounds,
    pub baseline: [f64; 2],
    pub font_size: f64,
    pub inferred_spaces: usize,
    pub issues: Vec<Reason>,
    pub kind: ElementKind,
    pub confidence: Confidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub kind: ElementKind,
    pub text: String,
    pub lines: Vec<String>,
    pub scene: Option<String>,
    /// Reference to the character-cue block, not an inferred actor/person ID.
    pub speaker: Option<String>,
    pub confidence: Confidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Doubt {
    pub line: String,
    pub block: String,
    pub suggested_kind: ElementKind,
    pub alternatives: Vec<ElementKind>,
    pub reasons: Vec<Reason>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screenplay {
    pub version: u32,
    pub coordinate_system: String,
    pub pages: Vec<Page>,
    pub lines: Vec<SourceLine>,
    pub blocks: Vec<Block>,
    pub doubts: Vec<Doubt>,
    pub warnings: Vec<Warning>,
}
