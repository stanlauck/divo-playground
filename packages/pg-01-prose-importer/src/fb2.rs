// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    emit::{plain, push_span, Engine},
    xml::{Child, Element, Node, Token, Xml},
    Block, Error, ImportOptions, ImportReport, Result, SourceFormat, Span, Stanza, TableCell,
    TableRow,
};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

#[derive(Default)]
struct Section {
    title: Option<String>,
    source_id: Option<String>,
    first_id: Option<String>,
    active_id: Option<String>,
}

/// Import FB2 in reading order. Nested sections form flat chapters with their
/// original depth. Parent text after a child is a linked continuation chapter.
pub fn import_fb2<R: BufRead, W: Write>(
    reader: R,
    writer: W,
    options: &ImportOptions,
) -> Result<ImportReport> {
    let mut engine = Engine::new(writer, SourceFormat::Fb2, options)?;
    let mut xml = Xml::new(reader, options, options.max_input_bytes);
    let mut root = false;
    let mut closed = false;
    let mut body = false;
    let mut seen_body = false;
    let mut notes = false;
    let mut sections = Vec::<Section>::new();
    let mut note_number = 0;
    loop {
        match xml.next()? {
            Token::Start(element) if !root => {
                if !element.name.fb("FictionBook") || closed {
                    return Err(Error::Invalid(
                        "expected one namespace-qualified FictionBook root",
                    ));
                }
                root = true;
            }
            Token::Start(element) if element.name.fb("body") && !closed => {
                if body {
                    return Err(Error::Invalid("nested FB2 body"));
                }
                body = true;
                seen_body = true;
                notes = matches!(element.attr("name"), Some("notes" | "comments"));
                sections.clear();
                engine.current = None;
            }
            Token::Start(element) if body && element.name.fb("section") => {
                if notes {
                    let id = element.attr("id").map(str::to_owned).unwrap_or_else(|| {
                        note_number += 1;
                        format!("fb2-note-{note_number}")
                    });
                    let (node, bytes) = xml.tree(element, options.max_block_bytes)?;
                    engine.source_block(bytes)?;
                    let blocks = children_blocks(&node, 1, &mut engine)?;
                    engine.block(Block::Footnote { id, blocks })?;
                } else {
                    if !sections.is_empty() {
                        ensure_section(&mut sections, &mut engine)?;
                    }
                    sections.push(Section {
                        source_id: element.attr("id").map(str::to_owned),
                        ..Section::default()
                    });
                    check_section_metadata(&sections, options)?;
                }
            }
            Token::Start(element) if body => {
                let (node, bytes) = xml.tree(element, options.max_block_bytes)?;
                engine.source_block(bytes)?;
                let level = sections.len() as u32;
                let blocks = convert(&node, level.max(1), &mut engine)?;
                if node.element.name.fb("title")
                    && !sections.is_empty()
                    && sections
                        .last()
                        .is_some_and(|section| section.first_id.is_none())
                {
                    let mut spans = Vec::new();
                    for block in &blocks {
                        if let Block::Heading { spans: text, .. } = block {
                            if !spans.is_empty() {
                                push_span(&mut spans, Span::default(), "\n");
                            }
                            spans.extend_from_slice(text);
                        }
                    }
                    let section = sections.last_mut().expect("section exists");
                    section.title = Some(plain(&spans));
                    check_section_metadata(&sections, options)?;
                }
                if !blocks.is_empty() {
                    ensure_section(&mut sections, &mut engine)?;
                    for block in blocks {
                        engine.block(block)?;
                    }
                }
            }
            Token::Start(element) => {
                if closed {
                    return Err(Error::Invalid("multiple FB2 roots"));
                }
                // Large binary elements are drained event-by-event, never
                // materialized as a source tree or decoded image.
                if element.name.fb("binary") {
                    engine.loss("fb2.binary_resource")?;
                } else {
                    engine.loss(format!("fb2.{}", element.name.local))?;
                }
                drain(&mut xml, element)?;
            }
            Token::End(name) if name.fb("section") => {
                if notes {
                    return Err(Error::Invalid("unexpected notes section end"));
                }
                if sections
                    .last()
                    .is_some_and(|section| section.first_id.is_none())
                {
                    ensure_section(&mut sections, &mut engine)?;
                }
                if sections.pop().is_none() {
                    return Err(Error::Invalid("section outside FB2 body"));
                }
            }
            Token::End(name) if name.fb("body") => {
                if !sections.is_empty() {
                    return Err(Error::Invalid("unclosed FB2 section"));
                }
                body = false;
                notes = false;
            }
            Token::End(name) if name.fb("FictionBook") => {
                if body {
                    return Err(Error::Invalid("unclosed FB2 body"));
                }
                closed = true;
            }
            Token::Text(text) if text.trim().is_empty() => {}
            Token::Eof => {
                if !root || !closed || !seen_body {
                    return Err(Error::Invalid("missing FB2 root"));
                }
                break;
            }
            _ => return Err(Error::Invalid("unexpected FB2 container content")),
        }
    }
    engine.finish()
}

fn ensure_section<W: Write>(sections: &mut [Section], engine: &mut Engine<W>) -> Result<()> {
    let level = sections.len() as u32;
    let Some(section) = sections.last_mut() else {
        return engine.ensure_chapter();
    };
    if section.first_id.is_none() || section.active_id.as_ref() != engine.current.as_ref() {
        let id = engine.chapter(
            level,
            section.title.clone(),
            section.source_id.clone(),
            section.first_id.clone(),
        )?;
        if section.first_id.is_none() {
            section.first_id = Some(id.clone());
        }
        section.active_id = Some(id);
    }
    Ok(())
}

fn check_section_metadata(sections: &[Section], options: &ImportOptions) -> Result<()> {
    let bytes = sections.iter().fold(0_usize, |sum, section| {
        sum.saturating_add(section.title.as_ref().map_or(0, String::len))
            .saturating_add(section.source_id.as_ref().map_or(0, String::len))
    });
    if bytes > options.max_metadata_bytes {
        return Err(Error::Limit("FB2 section metadata bytes"));
    }
    Ok(())
}

fn drain<R: BufRead>(xml: &mut Xml<R>, element: Element) -> Result<()> {
    let mut depth = 1_usize;
    while depth > 0 {
        match xml.next()? {
            Token::Start(_) => depth += 1,
            Token::End(name) => {
                depth -= 1;
                if depth == 0 && name != element.name {
                    return Err(Error::Invalid("invalid skipped FB2 element"));
                }
            }
            Token::Eof => return Err(Error::Invalid("truncated FB2 element")),
            _ => {}
        }
    }
    Ok(())
}

fn children_blocks<W: Write>(
    node: &Node,
    level: u32,
    engine: &mut Engine<W>,
) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    for child in node.nodes() {
        blocks.extend(convert(child, level, engine)?);
    }
    Ok(blocks)
}

fn convert<W: Write>(node: &Node, level: u32, engine: &mut Engine<W>) -> Result<Vec<Block>> {
    let name = &node.element.name;
    if name.ns != crate::xml::FB {
        engine.loss(format!("fb2.extension.{}", name.local))?;
        return children_blocks(node, level, engine);
    }
    let block = match name.local.as_str() {
        "p" | "subtitle" => {
            let spans = inline(node, Span::default(), engine)?;
            if name.local == "subtitle" {
                Block::Heading { level, spans }
            } else {
                Block::Paragraph { spans }
            }
        }
        "empty-line" => Block::Paragraph { spans: Vec::new() },
        "title" => {
            let mut blocks = Vec::new();
            for child in node.nodes() {
                blocks.push(Block::Heading {
                    level,
                    spans: inline(child, Span::default(), engine)?,
                });
            }
            return Ok(blocks);
        }
        "section" => return children_blocks(node, level + 1, engine),
        "cite" | "epigraph" => {
            let mut blocks = Vec::new();
            let mut attribution = Vec::new();
            for child in node.nodes() {
                if child.element.name.fb("text-author") {
                    if !attribution.is_empty() {
                        push_span(&mut attribution, Span::default(), "\n");
                    }
                    attribution.extend(inline(child, Span::default(), engine)?);
                } else {
                    blocks.extend(convert(child, level, engine)?);
                }
            }
            if name.local == "cite" {
                Block::Quote {
                    blocks,
                    attribution,
                }
            } else {
                Block::Epigraph {
                    blocks,
                    attribution,
                }
            }
        }
        "poem" => {
            let mut title = Vec::new();
            let mut epigraphs = Vec::new();
            let mut stanzas = Vec::new();
            let mut attribution = Vec::new();
            for child in node.nodes() {
                match child.element.name.local.as_str() {
                    "title" => title = title_spans(child, engine)?,
                    "epigraph" => epigraphs.extend(convert(child, level, engine)?),
                    "stanza" => {
                        let mut stanza = Stanza {
                            title: Vec::new(),
                            lines: Vec::new(),
                        };
                        for line in child.nodes() {
                            if line.element.name.fb("v") {
                                stanza.lines.push(inline(line, Span::default(), engine)?);
                            } else if line.element.name.fb("title") {
                                stanza.title = title_spans(line, engine)?;
                            } else {
                                engine.loss(format!("fb2.stanza.{}", line.element.name.local))?;
                            }
                        }
                        stanzas.push(stanza);
                    }
                    "text-author" => attribution = inline(child, Span::default(), engine)?,
                    other => engine.loss(format!("fb2.poem.{other}"))?,
                }
            }
            Block::Poem {
                title,
                epigraphs,
                stanzas,
                attribution,
            }
        }
        "table" => {
            let mut rows = Vec::new();
            let mut occupied = BTreeMap::<u32, (u32, u64)>::new();
            for row in node.nodes() {
                if !row.element.name.fb("tr") {
                    engine.loss(format!("fb2.table.{}", row.element.name.local))?;
                    continue;
                }
                let mut cells = Vec::new();
                let mut column = 0;
                let row_index = rows.len() as u64;
                occupied.retain(|_, (_, until)| *until > row_index);
                for cell in row.nodes() {
                    if !cell.element.name.fb("td") && !cell.element.name.fb("th") {
                        engine.loss(format!("fb2.table.{}", cell.element.name.local))?;
                        continue;
                    }
                    let colspan = dimension(cell.element.attr("colspan"))?;
                    let rowspan = dimension(cell.element.attr("rowspan"))?;
                    while let Some((_, (end, _))) = occupied.range(..=column).next_back() {
                        if column >= *end {
                            break;
                        }
                        column = *end;
                    }
                    let end = column
                        .checked_add(colspan)
                        .ok_or(Error::Limit("table columns"))?;
                    if occupied.range(column..end).next().is_some() {
                        return Err(Error::Invalid("overlapping FB2 table spans"));
                    }
                    cells.push(TableCell {
                        column,
                        colspan,
                        rowspan,
                        header: cell.element.name.fb("th"),
                        blocks: vec![Block::Paragraph {
                            spans: inline(cell, Span::default(), engine)?,
                        }],
                    });
                    if rowspan > 1 {
                        occupied.insert(column, (end, row_index + u64::from(rowspan)));
                    }
                    column = end;
                }
                rows.push(TableRow { cells });
            }
            Block::Table { rows }
        }
        "image" | "binary" => {
            engine.loss("fb2.images")?;
            return Ok(Vec::new());
        }
        "text-author" => {
            engine.loss("fb2.detached_attribution")?;
            Block::Paragraph {
                spans: inline(node, Span::default(), engine)?,
            }
        }
        _ => {
            engine.loss(format!("fb2.element.{}", name.local))?;
            let nested = children_blocks(node, level, engine)?;
            if !nested.is_empty() {
                return Ok(nested);
            }
            Block::Paragraph {
                spans: inline(node, Span::default(), engine)?,
            }
        }
    };
    if node.element.attr("style").is_some() {
        engine.loss("fb2.font_styles")?;
    }
    if node.element.attr("color").is_some() {
        engine.loss("fb2.colors")?;
    }
    Ok(vec![block])
}

fn dimension(value: Option<&str>) -> Result<u32> {
    value
        .unwrap_or("1")
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(Error::Invalid("table span must be positive"))
}

fn title_spans<W: Write>(node: &Node, engine: &mut Engine<W>) -> Result<Vec<Span>> {
    let mut spans = Vec::new();
    for child in node.nodes() {
        if !spans.is_empty() {
            push_span(&mut spans, Span::default(), "\n");
        }
        spans.extend(inline(child, Span::default(), engine)?);
    }
    Ok(spans)
}

fn inline<W: Write>(node: &Node, mut mark: Span, engine: &mut Engine<W>) -> Result<Vec<Span>> {
    if node.element.name.ns != crate::xml::FB {
        engine.loss(format!("fb2.extension.{}", node.element.name.local))?;
    } else {
        match node.element.name.local.as_str() {
            "strong" => mark.bold = true,
            "emphasis" => mark.italic = true,
            "strikethrough" => mark.strike = true,
            "sup" => {
                mark.superscript = true;
                mark.subscript = false;
            }
            "sub" => {
                mark.subscript = true;
                mark.superscript = false;
            }
            "a" => {
                mark.url = node.element.attr("href").map(str::to_owned);
                if node.element.attr("type") == Some("note") {
                    if let Some(id) = mark.url.as_deref().and_then(|href| href.strip_prefix('#')) {
                        if id.is_empty() {
                            engine.loss("fb2.invalid_note_reference")?;
                        } else {
                            mark.note_id = Some(id.into());
                        }
                    } else {
                        engine.loss("fb2.invalid_note_reference")?;
                    }
                }
            }
            "image" => {
                engine.loss("fb2.images")?;
                return Ok(Vec::new());
            }
            "style" | "code" => engine.loss("fb2.font_styles")?,
            "p" | "v" | "td" | "th" | "text-author" | "subtitle" | "title" => {}
            other => engine.loss(format!("fb2.inline.{other}"))?,
        }
    }
    if node.element.attr("color").is_some() {
        engine.loss("fb2.colors")?;
    }
    let mut spans = Vec::new();
    for child in &node.children {
        match child {
            Child::Text(text) => push_span(&mut spans, engine.clone_mark(&mark)?, text),
            Child::Node(child) => {
                let inherited = engine.clone_mark(&mark)?;
                for mut span in inline(child, inherited, engine)? {
                    let text = std::mem::take(&mut span.text);
                    push_span(&mut spans, span, &text);
                }
            }
        }
    }
    Ok(spans)
}
