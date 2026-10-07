// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Block, Error, ImportOptions, ImportReport, Loss, Result, SourceFormat, Span};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub(crate) struct Engine<W> {
    writer: W,
    pub options: ImportOptions,
    pub report: ImportReport,
    losses: BTreeMap<String, u64>,
    referenced: BTreeSet<String>,
    defined: BTreeSet<String>,
    metadata_bytes: usize,
    link_bytes: usize,
    pub current: Option<String>,
    has_chapter: bool,
    has_block: bool,
}

impl<W: Write> Engine<W> {
    pub fn new(mut writer: W, format: SourceFormat, options: &ImportOptions) -> Result<Self> {
        if options.max_input_bytes == 0
            || options.max_block_bytes == 0
            || options.max_metadata_bytes == 0
            || options.max_depth == 0
            || options.max_zip_entries == 0
            || options.max_metadata_entries == 0
        {
            return Err(Error::Invalid("all import limits must be positive"));
        }
        writer.write_all(b"{\"version\":1,\"source_format\":")?;
        serde_json::to_writer(&mut writer, &format)?;
        writer.write_all(b",\"chapters\":[")?;
        Ok(Self {
            writer,
            options: options.clone(),
            report: ImportReport::default(),
            losses: BTreeMap::new(),
            referenced: BTreeSet::new(),
            defined: BTreeSet::new(),
            metadata_bytes: 0,
            link_bytes: 0,
            current: None,
            has_chapter: false,
            has_block: false,
        })
    }

    pub fn chapter(
        &mut self,
        level: u32,
        title: Option<String>,
        source_id: Option<String>,
        continuation_of: Option<String>,
    ) -> Result<String> {
        if self.has_chapter {
            self.writer.write_all(b"]},")?;
        }
        self.report.chapters += 1;
        let id = format!("chapter-{}", self.report.chapters);
        self.writer.write_all(b"{\"id\":")?;
        serde_json::to_writer(&mut self.writer, &id)?;
        write!(self.writer, ",\"level\":{level},\"title\":")?;
        serde_json::to_writer(&mut self.writer, &title)?;
        self.writer.write_all(b",\"source_id\":")?;
        serde_json::to_writer(&mut self.writer, &source_id)?;
        self.writer.write_all(b",\"continuation_of\":")?;
        serde_json::to_writer(&mut self.writer, &continuation_of)?;
        self.writer.write_all(b",\"blocks\":[")?;
        self.current = Some(id.clone());
        self.has_chapter = true;
        self.has_block = false;
        Ok(id)
    }

    pub fn ensure_chapter(&mut self) -> Result<()> {
        if self.current.is_none() {
            self.chapter(0, None, None, None)?;
        }
        Ok(())
    }

    pub fn block(&mut self, block: Block) -> Result<()> {
        self.ensure_chapter()?;
        let mut size = Size::default();
        serde_json::to_writer(&mut size, &block)?;
        self.observe(size.0)?;
        self.count(&block)?;
        if self.has_block {
            self.writer.write_all(b",")?;
        }
        serde_json::to_writer(&mut self.writer, &block)?;
        self.has_block = true;
        Ok(())
    }

    pub fn observe(&mut self, bytes: usize) -> Result<()> {
        if bytes > self.options.max_block_bytes {
            return Err(Error::Limit("block bytes"));
        }
        self.report.peak_block_bytes = self.report.peak_block_bytes.max(bytes);
        Ok(())
    }

    pub fn source_block(&mut self, bytes: usize) -> Result<()> {
        self.link_bytes = 0;
        self.observe(bytes)
    }

    pub fn link_copy(&mut self, bytes: usize) -> Result<()> {
        self.link_bytes = self.link_bytes.saturating_add(bytes);
        if self.link_bytes > self.options.max_block_bytes {
            return Err(Error::Limit("expanded link bytes"));
        }
        Ok(())
    }

    pub fn clone_mark(&mut self, mark: &Span) -> Result<Span> {
        self.link_copy(
            mark.url
                .as_ref()
                .map_or(0, String::len)
                .saturating_add(mark.note_id.as_ref().map_or(0, String::len)),
        )?;
        Ok(mark.clone())
    }

    pub fn loss(&mut self, kind: impl Into<String>) -> Result<()> {
        let kind = kind.into();
        if !self.losses.contains_key(&kind)
            && self.losses.len() >= self.options.max_metadata_entries
        {
            return Err(Error::Limit("loss categories"));
        }
        if !self.losses.contains_key(&kind) {
            self.metadata(kind.len())?;
        }
        *self.losses.entry(kind).or_default() += 1;
        Ok(())
    }

    pub fn note(&mut self, id: &str) -> Result<()> {
        self.metadata(id.len())?;
        if id.is_empty() || !self.defined.insert(id.into()) {
            return Err(Error::Invalid("footnote IDs must be nonempty and unique"));
        }
        if self.defined.len() > self.options.max_metadata_entries {
            return Err(Error::Limit("footnote IDs"));
        }
        Ok(())
    }

    fn text(&mut self, spans: &[Span]) -> Result<()> {
        let mut inside_word = false;
        for span in spans {
            for character in span.text.chars() {
                if character.is_whitespace() {
                    inside_word = false;
                } else if !inside_word {
                    self.report.words += 1;
                    inside_word = true;
                }
            }
            if let Some(id) = &span.note_id {
                if !self.referenced.contains(id) {
                    self.metadata(id.len())?;
                }
                self.referenced.insert(id.clone());
                if self.referenced.len() > self.options.max_metadata_entries {
                    return Err(Error::Limit("footnote references"));
                }
            }
        }
        Ok(())
    }

    fn metadata(&mut self, bytes: usize) -> Result<()> {
        self.metadata_bytes = self.metadata_bytes.saturating_add(bytes);
        if self.metadata_bytes > self.options.max_metadata_bytes {
            return Err(Error::Limit("import-report metadata bytes"));
        }
        Ok(())
    }

    fn count(&mut self, block: &Block) -> Result<()> {
        self.report.blocks += 1;
        match block {
            Block::Paragraph { spans } | Block::Heading { spans, .. } => self.text(spans)?,
            Block::List { items, .. } => {
                for item in items {
                    for block in &item.blocks {
                        self.count(block)?;
                    }
                }
            }
            Block::Table { rows } => {
                for row in rows {
                    for cell in &row.cells {
                        for block in &cell.blocks {
                            self.count(block)?;
                        }
                    }
                }
            }
            Block::Quote {
                blocks,
                attribution,
            }
            | Block::Epigraph {
                blocks,
                attribution,
            } => {
                for block in blocks {
                    self.count(block)?;
                }
                self.text(attribution)?;
            }
            Block::Poem {
                title,
                epigraphs,
                stanzas,
                attribution,
            } => {
                self.text(title)?;
                for block in epigraphs {
                    self.count(block)?;
                }
                for stanza in stanzas {
                    self.text(&stanza.title)?;
                    for line in &stanza.lines {
                        self.text(line)?;
                    }
                }
                self.text(attribution)?;
            }
            Block::Footnote { id, blocks } => {
                self.note(id)?;
                for block in blocks {
                    self.count(block)?;
                }
            }
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<ImportReport> {
        if !self.has_chapter {
            self.chapter(0, None, None, None)?;
        }
        self.writer.write_all(b"]}],\"report\":")?;
        self.report.losses = self
            .losses
            .into_iter()
            .map(|(kind, occurrences)| Loss { kind, occurrences })
            .collect();
        self.report.unresolved_footnotes =
            self.referenced.difference(&self.defined).cloned().collect();
        serde_json::to_writer(&mut self.writer, &self.report)?;
        self.writer.write_all(b"}")?;
        Ok(self.report)
    }
}

#[derive(Default)]
struct Size(usize);
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn plain(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

pub(crate) fn push_span(spans: &mut Vec<Span>, mut mark: Span, text: &str) {
    if text.is_empty() {
        return;
    }
    mark.text.clear();
    if let Some(last) = spans.last_mut() {
        if last.bold == mark.bold
            && last.italic == mark.italic
            && last.strike == mark.strike
            && last.superscript == mark.superscript
            && last.subscript == mark.subscript
            && last.url == mark.url
            && last.note_id == mark.note_id
        {
            last.text.push_str(text);
            return;
        }
    }
    mark.text.push_str(text);
    spans.push(mark);
}
