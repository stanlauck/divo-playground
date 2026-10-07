// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Fb2,
    Docx,
    Txt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Document {
    pub version: u8,
    pub source_format: SourceFormat,
    pub chapters: Vec<Chapter>,
    pub report: ImportReport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Chapter {
    pub id: String,
    pub level: u32,
    pub title: Option<String>,
    pub source_id: Option<String>,
    /// Reading-order continuation after a nested FB2 section.
    pub continuation_of: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Span {
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strike: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub superscript: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub subscript: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        spans: Vec<Span>,
    },
    Heading {
        level: u32,
        spans: Vec<Span>,
    },
    List {
        id: String,
        ordered: bool,
        start: u64,
        number_format: String,
        marker: Option<String>,
        items: Vec<ListItem>,
    },
    Table {
        rows: Vec<TableRow>,
    },
    Quote {
        blocks: Vec<Block>,
        attribution: Vec<Span>,
    },
    Epigraph {
        blocks: Vec<Block>,
        attribution: Vec<Span>,
    },
    Poem {
        title: Vec<Span>,
        epigraphs: Vec<Block>,
        stanzas: Vec<Stanza>,
        attribution: Vec<Span>,
    },
    Footnote {
        id: String,
        blocks: Vec<Block>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ListItem {
    /// Zero-based nesting level.
    pub level: u32,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Stanza {
    pub title: Vec<Span>,
    pub lines: Vec<Vec<Span>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TableCell {
    /// Zero-based grid column, including cells covered by a vertical merge.
    pub column: u32,
    pub colspan: u32,
    pub rowspan: u32,
    pub header: bool,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loss {
    pub kind: String,
    pub occurrences: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportReport {
    pub chapters: u64,
    /// Includes nested blocks, but not individual spans or verse lines.
    pub blocks: u64,
    pub words: u64,
    pub losses: Vec<Loss>,
    pub unresolved_footnotes: Vec<String>,
    /// Logical source-tree/block text bytes, not total allocator/RSS usage.
    pub peak_block_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub max_input_bytes: u64,
    pub max_block_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_depth: usize,
    pub max_zip_entries: usize,
    pub max_metadata_entries: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * 1024 * 1024,
            max_block_bytes: 8 * 1024 * 1024,
            max_metadata_bytes: 4 * 1024 * 1024,
            max_depth: 128,
            max_zip_entries: 10_000,
            max_metadata_entries: 100_000,
        }
    }
}
