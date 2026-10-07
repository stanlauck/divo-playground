// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    emit::{plain, push_span, Engine},
    xml::{Node, Token, Xml},
    Block, Error, ImportOptions, ImportReport, ListItem, Result, SourceFormat, Span, TableCell,
    TableRow,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use zip::ZipArchive;

#[derive(Clone, Default)]
struct Props {
    bold: Option<bool>,
    italic: Option<bool>,
    strike: Option<bool>,
    superscript: Option<bool>,
    subscript: Option<bool>,
    heading: Option<u32>,
    num: Option<String>,
    list_level: Option<u32>,
    quote: bool,
    losses: BTreeSet<String>,
}
impl Props {
    fn overlay(&mut self, other: &Self) {
        macro_rules! set { ($($field:ident),*) => { $(if other.$field.is_some() { self.$field = other.$field.clone(); })* }; }
        set!(
            bold,
            italic,
            strike,
            superscript,
            subscript,
            heading,
            num,
            list_level
        );
        self.quote |= other.quote;
        self.losses.extend(other.losses.iter().cloned());
    }
    fn mark(&self) -> Span {
        Span {
            bold: self.bold.unwrap_or(false),
            italic: self.italic.unwrap_or(false),
            strike: self.strike.unwrap_or(false),
            superscript: self.superscript.unwrap_or(false),
            subscript: self.subscript.unwrap_or(false),
            ..Span::default()
        }
    }
    fn overlay_style(&mut self, other: &Self) {
        let previous = (self.bold, self.italic, self.strike);
        self.overlay(other);
        self.bold = if other.bold == Some(true) {
            Some(!previous.0.unwrap_or(false))
        } else {
            previous.0
        };
        self.italic = if other.italic == Some(true) {
            Some(!previous.1.unwrap_or(false))
        } else {
            previous.1
        };
        self.strike = if other.strike == Some(true) {
            Some(!previous.2.unwrap_or(false))
        } else {
            previous.2
        };
    }
}

struct Style {
    parent: Option<String>,
    props: Props,
}
#[derive(Clone)]
struct Numbering {
    start: u64,
    format: String,
    marker: Option<String>,
}
impl Numbering {
    fn bytes(&self, id: &str) -> usize {
        id.len()
            .saturating_add(self.format.len())
            .saturating_add(self.marker.as_ref().map_or(0, String::len))
            .saturating_add(16)
    }
}
#[derive(Default)]
struct Context {
    styles: BTreeMap<String, Style>,
    defaults: Props,
    default_style: Option<String>,
    links: BTreeMap<String, String>,
    nums: BTreeMap<(String, u32), Numbering>,
    counters: BTreeMap<String, BTreeMap<u32, u64>>,
    numbering_bytes: usize,
}
impl Context {
    fn number(
        &mut self,
        id: &str,
        level: u32,
        definition: &Numbering,
        options: &ImportOptions,
    ) -> Result<()> {
        let key = (id.to_owned(), level);
        let replaced = self.nums.get(&key).map_or(0, |old| old.bytes(id));
        let bytes = self
            .numbering_bytes
            .saturating_sub(replaced)
            .saturating_add(definition.bytes(id));
        if bytes > options.max_metadata_bytes {
            return Err(Error::Limit("expanded numbering bytes"));
        }
        if !self.nums.contains_key(&key) && self.nums.len() >= options.max_metadata_entries {
            return Err(Error::Limit("numbering definitions"));
        }
        self.nums.insert(key, definition.clone());
        self.numbering_bytes = bytes;
        Ok(())
    }

    fn advance(&mut self, id: &str, level: u32) -> Result<(u64, &Numbering)> {
        let definition = self
            .nums
            .get(&(id.to_owned(), level))
            .ok_or(Error::Invalid("undefined list numbering"))?;
        let counters = self.counters.entry(id.to_owned()).or_default();
        let start = counters.get(&level).copied().unwrap_or(definition.start);
        let next = start
            .checked_add(1)
            .ok_or(Error::Limit("list item count"))?;
        counters.insert(level, next);
        counters.retain(|nested, _| *nested <= level);
        Ok((start, definition))
    }
}

/// Import a DOCX ZIP without extracting files or fetching external resources.
/// Main document blocks are streamed; styles, numbering and relationships are
/// individually bounded metadata. Footnotes are streamed from their own part.
pub fn import_docx<R: Read + Seek, W: Write>(
    mut reader: R,
    writer: W,
    options: &ImportOptions,
) -> Result<ImportReport> {
    let length = reader.seek(SeekFrom::End(0))?;
    if length > options.max_input_bytes {
        return Err(Error::Limit("DOCX archive bytes"));
    }
    reader.seek(SeekFrom::Start(0))?;
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() > options.max_zip_entries {
        return Err(Error::Limit("ZIP entries"));
    }
    let mut engine = Engine::new(writer, SourceFormat::Docx, options)?;
    let mut context = Context::default();
    if let Some(styles) = part_tree(&mut archive, "word/styles.xml", options)? {
        parse_styles(&styles, &mut context, options)?;
    }
    if let Some(numbering) = part_tree(&mut archive, "word/numbering.xml", options)? {
        parse_numbering(&numbering, &mut context, options)?;
    }
    if let Some(links) = part_tree(&mut archive, "word/_rels/document.xml.rels", options)? {
        context.links = relationships(&links, options)?;
    }
    for name in archive.file_names() {
        if name.starts_with("word/media/") {
            engine.loss("docx.images")?;
        } else if name.starts_with("word/header") || name.starts_with("word/footer") {
            engine.loss("docx.headers_footers")?;
        } else if name.starts_with("word/theme") {
            engine.loss("docx.theme")?;
        } else if name.starts_with("word/embeddings/") {
            engine.loss("docx.embedded_objects")?;
        } else if name.starts_with("word/comments") {
            engine.loss("docx.comments")?;
        } else if name.starts_with("word/endnotes") {
            engine.loss("docx.endnotes")?;
        } else if name.starts_with("word/fontTable") {
            engine.loss("docx.font_styles")?;
        } else if name.starts_with("docProps/") {
            engine.loss("docx.package_metadata")?;
        } else if name == "word/settings.xml" {
            engine.loss("docx.document_settings")?;
        }
    }
    {
        let file = archive.by_name("word/document.xml")?;
        if file.size() > options.max_input_bytes {
            return Err(Error::Limit("DOCX document bytes"));
        }
        let mut xml = Xml::new(BufReader::new(file), options, options.max_input_bytes);
        parse_document(&mut xml, &mut context, &mut engine)?;
    }
    context.links = match part_tree(&mut archive, "word/_rels/footnotes.xml.rels", options)? {
        Some(links) => relationships(&links, options)?,
        None => BTreeMap::new(),
    };
    let notes = match archive.by_name("word/footnotes.xml") {
        Ok(file) => Some(file),
        Err(zip::result::ZipError::FileNotFound) => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(file) = notes {
        if file.size() > options.max_input_bytes {
            return Err(Error::Limit("DOCX footnote bytes"));
        }
        let mut xml = Xml::new(BufReader::new(file), options, options.max_input_bytes);
        engine.current = None;
        let mut root = false;
        let mut closed = false;
        loop {
            match xml.next()? {
                Token::Start(element) if !root => {
                    if !element.name.word("footnotes") {
                        return Err(Error::Invalid("invalid DOCX footnotes root"));
                    }
                    root = true;
                }
                Token::Start(element) if element.name.word("footnote") && !closed => {
                    let (note, bytes) = xml.tree(element, options.max_block_bytes)?;
                    engine.source_block(bytes)?;
                    if matches!(
                        note.element.attr("type"),
                        Some("separator" | "continuationSeparator")
                    ) {
                        engine.loss("docx.footnote_separator")?;
                        continue;
                    }
                    let id = note
                        .element
                        .attr("id")
                        .ok_or(Error::Invalid("footnote has no ID"))?;
                    let mut blocks = Vec::new();
                    for child in note.nodes() {
                        blocks.extend(blocks_for(child, &mut context, &mut engine)?);
                    }
                    engine.block(Block::Footnote {
                        id: format!("docx-footnote-{id}"),
                        blocks,
                    })?;
                }
                Token::End(name) if name.word("footnotes") => closed = true,
                Token::Text(text) if text.trim().is_empty() => {}
                Token::Eof if root && closed => break,
                _ => return Err(Error::Invalid("unexpected DOCX footnotes content")),
            }
        }
    }
    engine.finish()
}

fn part_tree<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    options: &ImportOptions,
) -> Result<Option<Node>> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if file.size() > options.max_metadata_bytes as u64 {
        return Err(Error::Limit("DOCX metadata bytes"));
    }
    let mut xml = Xml::new(
        BufReader::new(file),
        options,
        options.max_metadata_bytes as u64,
    );
    let start = loop {
        match xml.next()? {
            Token::Start(start) => break start,
            Token::Text(text) if text.trim().is_empty() => {}
            _ => return Err(Error::Invalid("missing DOCX metadata root")),
        }
    };
    let (node, _) = xml.tree(start, options.max_metadata_bytes)?;
    loop {
        match xml.next()? {
            Token::Text(text) if text.trim().is_empty() => {}
            Token::Eof => break,
            _ => return Err(Error::Invalid("multiple DOCX metadata roots")),
        }
    }
    Ok(Some(node))
}

fn parse_document<R: Read, W: Write>(
    xml: &mut Xml<R>,
    context: &mut Context,
    engine: &mut Engine<W>,
) -> Result<()> {
    let mut root = false;
    let mut body = false;
    let mut seen_body = false;
    let mut closed = false;
    loop {
        match xml.next()? {
            Token::Start(element) if !root => {
                if !element.name.word("document") {
                    return Err(Error::Invalid("invalid DOCX document root"));
                }
                root = true;
            }
            Token::Start(element) if element.name.word("body") && !closed => {
                if body || seen_body {
                    return Err(Error::Invalid("multiple or nested DOCX bodies"));
                }
                body = true;
                seen_body = true;
            }
            Token::Start(element) if body => {
                let (node, bytes) = xml.tree(element, engine.options.max_block_bytes)?;
                engine.source_block(bytes)?;
                for block in blocks_for(&node, context, engine)? {
                    if let Block::Heading { level, spans } = &block {
                        engine.chapter(*level, Some(plain(spans)), None, None)?;
                    }
                    engine.block(block)?;
                }
            }
            Token::End(name) if name.word("body") => body = false,
            Token::End(name) if name.word("document") => closed = true,
            Token::Text(text) if text.trim().is_empty() => {}
            Token::Eof if root && closed && seen_body => break,
            _ => return Err(Error::Invalid("unexpected DOCX document content")),
        }
    }
    Ok(())
}

fn toggle(node: &Node) -> Result<bool> {
    match node.element.attr("val").unwrap_or("1") {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => Err(Error::Invalid("invalid Word boolean")),
    }
}

fn props(node: &Node) -> Result<Props> {
    let mut result = Props::default();
    for child in node.nodes() {
        if !matches!(
            child.element.name.ns.as_str(),
            crate::xml::WORD | crate::xml::STRICT_WORD
        ) {
            result
                .losses
                .insert(format!("docx.extension.{}", child.element.name.local));
            continue;
        }
        let value = child.element.attr("val");
        match child.element.name.local.as_str() {
            "b" => result.bold = Some(toggle(child)?),
            "i" => result.italic = Some(toggle(child)?),
            "bCs" | "iCs" | "cs" | "rtl" => {
                toggle(child)?;
                result
                    .losses
                    .insert("docx.complex_script_formatting".into());
            }
            "strike" | "dstrike" => result.strike = Some(toggle(child)?),
            "vertAlign" => match value {
                Some("superscript") => {
                    result.superscript = Some(true);
                    result.subscript = Some(false);
                }
                Some("subscript") => {
                    result.subscript = Some(true);
                    result.superscript = Some(false);
                }
                Some("baseline") => {
                    result.subscript = Some(false);
                    result.superscript = Some(false);
                }
                _ => return Err(Error::Invalid("invalid Word vertical alignment")),
            },
            "outlineLvl" => {
                let value = value
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|v| *v <= 9)
                    .ok_or(Error::Invalid("invalid Word outline level"))?;
                result.heading = Some(if value == 9 { 0 } else { value + 1 });
            }
            "numPr" => {
                result.num = child
                    .child_word("numId")
                    .and_then(|n| n.element.attr("val"))
                    .map(str::to_owned);
                result.list_level = child
                    .child_word("ilvl")
                    .and_then(|n| n.element.attr("val"))
                    .map(|v| {
                        v.parse::<u32>()
                            .map_err(|_| Error::Invalid("invalid list level"))
                    })
                    .transpose()?;
            }
            "pStyle" | "rStyle" => {}
            "rPr" if node.element.name.word("pPr") => {
                result.losses.insert("docx.paragraph_mark_format".into());
            }
            "rPr" | "pPr" => result.overlay(&props(child)?),
            "rFonts" | "sz" | "szCs" => {
                result.losses.insert("docx.font_styles".into());
            }
            "color" | "highlight" | "shd" => {
                result.losses.insert("docx.colors".into());
            }
            other => {
                result.losses.insert(format!("docx.format.{other}"));
            }
        }
    }
    Ok(result)
}

fn parse_styles(node: &Node, context: &mut Context, options: &ImportOptions) -> Result<()> {
    if !node.element.name.word("styles") {
        return Err(Error::Invalid("invalid styles root"));
    }
    for child in node.nodes() {
        if child.element.name.word("docDefaults") {
            for defaults in child.nodes() {
                context.defaults.overlay(&props(defaults)?);
            }
        } else if child.element.name.word("style") {
            let id = child
                .element
                .attr("styleId")
                .ok_or(Error::Invalid("style has no ID"))?;
            let mut properties = Props::default();
            for property in child.nodes() {
                if property.element.name.word("pPr") || property.element.name.word("rPr") {
                    properties.overlay(&props(property)?);
                }
            }
            let display_name = child
                .child_word("name")
                .and_then(|n| n.element.attr("val"))
                .unwrap_or(id);
            let normalized = display_name.to_ascii_lowercase().replace(' ', "");
            if properties.heading.is_none() {
                if let Some(level) = heading_level(display_name).or_else(|| heading_level(id)) {
                    properties.heading = Some(level);
                }
            }
            properties.quote = matches!(normalized.as_str(), "quote" | "intensequote");
            let parent = child
                .child_word("basedOn")
                .and_then(|n| n.element.attr("val"))
                .map(str::to_owned);
            if matches!(child.element.attr("default"), Some("1" | "true" | "on"))
                && child.element.attr("type") == Some("paragraph")
            {
                context.default_style = Some(id.into());
            }
            if context
                .styles
                .insert(
                    id.into(),
                    Style {
                        parent,
                        props: properties,
                    },
                )
                .is_some()
            {
                return Err(Error::Invalid("duplicate Word style ID"));
            }
            if context.styles.len() > options.max_metadata_entries {
                return Err(Error::Limit("Word styles"));
            }
        }
    }
    Ok(())
}

fn heading_level(name: &str) -> Option<u32> {
    name.to_ascii_lowercase()
        .replace(' ', "")
        .strip_prefix("heading")
        .and_then(|n| n.parse().ok())
        .filter(|n| (1..=9).contains(n))
}

fn style(id: &str, context: &Context, limit: usize) -> Result<Props> {
    let mut chain = Vec::new();
    let mut visited = BTreeSet::new();
    let mut id = Some(id);
    while let Some(key) = id {
        if chain.len() >= limit || !visited.insert(key) {
            return Err(Error::Invalid("cyclic or excessive style inheritance"));
        }
        let Some(definition) = context.styles.get(key) else {
            break;
        };
        chain.push(&definition.props);
        id = definition.parent.as_deref();
    }
    let mut result = Props::default();
    for properties in chain.into_iter().rev() {
        result.overlay_style(properties);
    }
    Ok(result)
}

fn parse_numbering(node: &Node, context: &mut Context, options: &ImportOptions) -> Result<()> {
    if !node.element.name.word("numbering") {
        return Err(Error::Invalid("invalid numbering root"));
    }
    let mut abstracts = BTreeMap::new();
    let mut abstract_bytes = 0_usize;
    for abstract_node in node.nodes().filter(|n| n.element.name.word("abstractNum")) {
        let id = abstract_node
            .element
            .attr("abstractNumId")
            .ok_or(Error::Invalid("abstract numbering has no ID"))?;
        for level in abstract_node.nodes().filter(|n| n.element.name.word("lvl")) {
            let index = level
                .element
                .attr("ilvl")
                .and_then(|n| n.parse::<u32>().ok())
                .ok_or(Error::Invalid("invalid numbering level"))?;
            let definition = numbering_level(level)?;
            abstract_bytes = abstract_bytes.saturating_add(definition.bytes(id));
            if abstract_bytes > options.max_metadata_bytes {
                return Err(Error::Limit("abstract numbering bytes"));
            }
            if abstracts
                .insert((id.to_owned(), index), definition)
                .is_some()
            {
                return Err(Error::Invalid("duplicate abstract numbering level"));
            }
            if abstracts.len() > options.max_metadata_entries {
                return Err(Error::Limit("numbering levels"));
            }
        }
    }
    for num in node.nodes().filter(|n| n.element.name.word("num")) {
        let id = num
            .element
            .attr("numId")
            .ok_or(Error::Invalid("numbering has no ID"))?;
        let abstract_id = num
            .child_word("abstractNumId")
            .and_then(|n| n.element.attr("val"))
            .ok_or(Error::Invalid("numbering has no abstract ID"))?;
        for ((_, level), definition) in
            abstracts.range((abstract_id.to_owned(), 0)..=(abstract_id.to_owned(), u32::MAX))
        {
            context.number(id, *level, definition, options)?;
        }
        for override_node in num.nodes().filter(|n| n.element.name.word("lvlOverride")) {
            let level = override_node
                .element
                .attr("ilvl")
                .and_then(|n| n.parse::<u32>().ok())
                .ok_or(Error::Invalid("invalid numbering override"))?;
            let key = (id.to_owned(), level);
            if let Some(definition) = override_node.child_word("lvl") {
                context.number(id, level, &numbering_level(definition)?, options)?;
            }
            if let Some(start) = override_node.child_word("startOverride") {
                let start = start
                    .element
                    .attr("val")
                    .and_then(|v| v.parse::<u64>().ok())
                    .ok_or(Error::Invalid("invalid list start"))?;
                context
                    .nums
                    .get_mut(&key)
                    .ok_or(Error::Invalid("unknown numbering override"))?
                    .start = start;
            }
        }
        if context.nums.len() > options.max_metadata_entries {
            return Err(Error::Limit("numbering definitions"));
        }
    }
    Ok(())
}

fn numbering_level(node: &Node) -> Result<Numbering> {
    let start = node
        .child_word("start")
        .and_then(|n| n.element.attr("val"))
        .map(|v| {
            v.parse::<u64>()
                .map_err(|_| Error::Invalid("invalid list start"))
        })
        .transpose()?
        .unwrap_or(1);
    Ok(Numbering {
        start,
        format: node
            .child_word("numFmt")
            .and_then(|n| n.element.attr("val"))
            .unwrap_or("decimal")
            .into(),
        marker: node
            .child_word("lvlText")
            .and_then(|n| n.element.attr("val"))
            .map(str::to_owned),
    })
}

fn relationships(node: &Node, options: &ImportOptions) -> Result<BTreeMap<String, String>> {
    if node.element.name.local != "Relationships"
        || node.element.name.ns != "http://schemas.openxmlformats.org/package/2006/relationships"
    {
        return Err(Error::Invalid("invalid relationships root"));
    }
    let mut result = BTreeMap::new();
    for relationship in node.nodes() {
        if relationship.element.name.local != "Relationship"
            || relationship.element.name.ns != node.element.name.ns
        {
            return Err(Error::Invalid("invalid package relationship"));
        }
        if relationship
            .element
            .attr("Type")
            .is_some_and(|kind| kind.ends_with("/hyperlink"))
        {
            let id = relationship
                .element
                .attr("Id")
                .ok_or(Error::Invalid("relationship has no ID"))?;
            let target = relationship
                .element
                .attr("Target")
                .ok_or(Error::Invalid("relationship has no target"))?;
            if result.insert(id.into(), target.into()).is_some() {
                return Err(Error::Invalid("duplicate hyperlink ID"));
            }
            if result.len() > options.max_metadata_entries {
                return Err(Error::Limit("hyperlinks"));
            }
        }
    }
    Ok(result)
}

fn blocks_for<W: Write>(
    node: &Node,
    context: &mut Context,
    engine: &mut Engine<W>,
) -> Result<Vec<Block>> {
    if node.element.name.word("p") {
        return Ok(vec![paragraph(node, context, engine)?]);
    }
    if node.element.name.word("tbl") {
        return Ok(vec![table(node, context, engine)?]);
    }
    if node.element.name.word("sectPr") {
        engine.loss("docx.page_layout")?;
        return Ok(Vec::new());
    }
    if matches!(node.element.name.local.as_str(), "del" | "moveFrom") {
        engine.loss("docx.deleted_revision")?;
        return Ok(Vec::new());
    }
    engine.loss(format!("docx.container.{}", node.element.name.local))?;
    let mut blocks = Vec::new();
    for child in node.nodes() {
        blocks.extend(blocks_for(child, context, engine)?);
    }
    Ok(blocks)
}

fn paragraph<W: Write>(
    node: &Node,
    context: &mut Context,
    engine: &mut Engine<W>,
) -> Result<Block> {
    let ppr = node.child_word("pPr");
    let mut properties = context.defaults.clone();
    let paragraph_style = ppr
        .and_then(|n| n.child_word("pStyle"))
        .and_then(|n| n.element.attr("val"))
        .or(context.default_style.as_deref());
    if let Some(id) = paragraph_style {
        properties.overlay_style(&style(id, context, engine.options.max_depth)?);
        if !context.styles.contains_key(id) {
            let normalized = id.to_ascii_lowercase();
            if let Some(level) = heading_level(id) {
                properties.heading = Some(level);
            } else if normalized == "quote" {
                properties.quote = true;
            } else {
                properties.losses.insert("docx.unknown_style".into());
            }
        }
    }
    if let Some(ppr) = ppr {
        properties.overlay(&props(ppr)?);
    }
    for loss in &properties.losses {
        engine.loss(loss.clone())?;
    }
    let mut spans = Vec::new();
    for child in node.nodes().filter(|n| !n.element.name.word("pPr")) {
        spans.extend(inline(child, &properties, context, engine)?);
    }
    if let Some(level) = properties.heading.filter(|level| *level > 0) {
        if let Some(id) = properties.num.as_deref().filter(|id| *id != "0") {
            context.advance(id, properties.list_level.unwrap_or(0))?;
            engine.loss("docx.heading_numbering")?;
        }
        return Ok(Block::Heading { level, spans });
    }
    let paragraph = Block::Paragraph { spans };
    if let Some(id) = properties.num.filter(|id| id != "0") {
        let level = properties.list_level.unwrap_or(0);
        let (start, definition) = context.advance(&id, level)?;
        engine.list_copy(definition.bytes(&id))?;
        let paragraph = if properties.quote {
            Block::Quote {
                blocks: vec![paragraph],
                attribution: Vec::new(),
            }
        } else {
            paragraph
        };
        return Ok(Block::List {
            id: format!("docx-num-{id}"),
            ordered: !matches!(definition.format.as_str(), "bullet" | "none"),
            start,
            number_format: definition.format.clone(),
            marker: definition.marker.clone(),
            items: vec![ListItem {
                level,
                blocks: vec![paragraph],
            }],
        });
    }
    if properties.quote {
        return Ok(Block::Quote {
            blocks: vec![paragraph],
            attribution: Vec::new(),
        });
    }
    Ok(paragraph)
}

fn inline<W: Write>(
    node: &Node,
    inherited: &Props,
    context: &Context,
    engine: &mut Engine<W>,
) -> Result<Vec<Span>> {
    let local = node.element.name.local.as_str();
    if !matches!(
        node.element.name.ns.as_str(),
        crate::xml::WORD | crate::xml::STRICT_WORD
    ) {
        if matches!(local, "pict" | "drawing" | "object") {
            engine.loss("docx.images")?;
            return Ok(Vec::new());
        }
        engine.loss(format!("docx.extension.{local}"))?;
        let mut spans = Vec::new();
        for child in node.nodes() {
            spans.extend(inline(child, inherited, context, engine)?);
        }
        return Ok(spans);
    }
    if matches!(local, "drawing" | "pict" | "object") {
        engine.loss("docx.images")?;
        return Ok(Vec::new());
    }
    if matches!(local, "del" | "moveFrom") {
        engine.loss("docx.deleted_revision")?;
        return Ok(Vec::new());
    }
    if node.element.name.word("r") {
        let mut properties = inherited.clone();
        properties.losses.clear();
        if let Some(rpr) = node.child_word("rPr") {
            if let Some(id) = rpr.child_word("rStyle").and_then(|n| n.element.attr("val")) {
                properties.overlay_style(&style(id, context, engine.options.max_depth)?);
            }
            properties.overlay(&props(rpr)?);
        }
        for loss in &properties.losses {
            engine.loss(loss.clone())?;
        }
        let mark = properties.mark();
        let mut spans = Vec::new();
        for child in node.nodes().filter(|n| !n.element.name.word("rPr")) {
            if !matches!(
                child.element.name.ns.as_str(),
                crate::xml::WORD | crate::xml::STRICT_WORD
            ) {
                spans.extend(inline(child, &properties, context, engine)?);
                continue;
            }
            match child.element.name.local.as_str() {
                "t" => push_span(&mut spans, mark.clone(), &child.text()),
                "tab" => push_span(&mut spans, mark.clone(), "\t"),
                "br" | "cr" => {
                    if child
                        .element
                        .attr("type")
                        .is_some_and(|kind| kind != "textWrapping")
                    {
                        engine.loss("docx.page_column_break")?;
                    }
                    push_span(&mut spans, mark.clone(), "\n");
                }
                "noBreakHyphen" => push_span(&mut spans, mark.clone(), "\u{2011}"),
                "softHyphen" => push_span(&mut spans, mark.clone(), "\u{00ad}"),
                "footnoteReference" => {
                    let id = child
                        .element
                        .attr("id")
                        .ok_or(Error::Invalid("footnote reference has no ID"))?;
                    let mut note_mark = mark.clone();
                    note_mark.note_id = Some(format!("docx-footnote-{id}"));
                    note_mark.superscript = true;
                    note_mark.subscript = false;
                    push_span(&mut spans, note_mark, id);
                }
                "footnoteRef" => {} // implicit label inside a footnote, not its prose
                "drawing" | "pict" | "object" => engine.loss("docx.images")?,
                "instrText" | "fldChar" => engine.loss("docx.field_code")?,
                other => engine.loss(format!("docx.run.{other}"))?,
            }
        }
        return Ok(spans);
    }
    let mut spans = Vec::new();
    for child in node.nodes() {
        spans.extend(inline(child, inherited, context, engine)?);
    }
    if node.element.name.word("hyperlink") {
        let url = if let Some(anchor) = node.element.attr("anchor") {
            Some(format!("#{anchor}"))
        } else {
            node.element
                .attr("id")
                .and_then(|id| context.links.get(id))
                .cloned()
        };
        if url.is_none() {
            engine.loss("docx.unresolved_hyperlink")?;
        }
        for span in &mut spans {
            engine.link_copy(url.as_ref().map_or(0, String::len))?;
            span.url = url.clone();
        }
    } else if !matches!(local, "ins" | "moveTo" | "sdtContent") {
        engine.loss(format!("docx.inline.{local}"))?;
    } else if matches!(local, "ins" | "moveTo") {
        engine.loss("docx.revision_metadata")?;
    }
    Ok(spans)
}

fn table<W: Write>(node: &Node, context: &mut Context, engine: &mut Engine<W>) -> Result<Block> {
    let mut rows: Vec<TableRow> = Vec::new();
    let mut vertical: BTreeMap<u32, (usize, usize, u32)> = BTreeMap::new();
    for child in node.nodes() {
        if !child.element.name.word("tr") {
            engine.loss(format!("docx.table.{}", child.element.name.local))?;
            continue;
        }
        let row_index = rows.len();
        let row_props = child.child_word("trPr");
        let header = row_props
            .and_then(|n| n.child_word("tblHeader"))
            .map(toggle)
            .transpose()?
            .unwrap_or(false);
        let mut column = row_props
            .and_then(|n| n.child_word("gridBefore"))
            .and_then(|n| n.element.attr("val"))
            .map(|v| {
                v.parse::<u32>()
                    .map_err(|_| Error::Invalid("invalid table grid offset"))
            })
            .transpose()?
            .unwrap_or(0);
        let mut cells = Vec::new();
        let mut retained = BTreeSet::new();
        for cell in child.nodes().filter(|n| n.element.name.word("tc")) {
            let properties = cell.child_word("tcPr");
            let colspan = properties
                .and_then(|n| n.child_word("gridSpan"))
                .and_then(|n| n.element.attr("val"))
                .map(|v| {
                    v.parse::<u32>()
                        .map_err(|_| Error::Invalid("invalid grid span"))
                })
                .transpose()?
                .unwrap_or(1);
            if colspan == 0 {
                return Err(Error::Invalid("zero grid span"));
            }
            let merge = properties.and_then(|n| n.child_word("vMerge"));
            let continuation =
                merge.is_some_and(|n| n.element.attr("val").unwrap_or("continue") == "continue");
            let mut blocks = Vec::new();
            for content in cell.nodes().filter(|n| !n.element.name.word("tcPr")) {
                blocks.extend(blocks_for(content, context, engine)?);
            }
            if let Some(properties) = properties {
                for format in properties.nodes() {
                    if !format.element.name.word("gridSpan") && !format.element.name.word("vMerge")
                    {
                        engine.loss(format!("docx.cell_format.{}", format.element.name.local))?;
                    }
                }
            }
            if continuation {
                let (origin_row, origin_cell, width) = vertical
                    .get(&column)
                    .copied()
                    .ok_or(Error::Invalid("vertical merge has no start cell"))?;
                if width != colspan {
                    return Err(Error::Invalid("vertical merge changes width"));
                }
                rows[origin_row].cells[origin_cell].rowspan += 1;
                // A continuation's required empty paragraph is structural.
                for block in blocks {
                    if !matches!(&block, Block::Paragraph { spans } if spans.iter().all(|s| s.text.is_empty()))
                    {
                        rows[origin_row].cells[origin_cell].blocks.push(block);
                    }
                }
                retained.insert(column);
            } else {
                vertical.remove(&column);
                if let Some(merge) = merge {
                    if merge.element.attr("val") != Some("restart") {
                        return Err(Error::Invalid("invalid vertical merge"));
                    }
                    vertical.insert(column, (row_index, cells.len(), colspan));
                    retained.insert(column);
                }
                cells.push(TableCell {
                    column,
                    colspan,
                    rowspan: 1,
                    header,
                    blocks,
                });
            }
            column = column
                .checked_add(colspan)
                .ok_or(Error::Limit("table columns"))?;
        }
        vertical.retain(|column, _| retained.contains(column));
        rows.push(TableRow { cells });
    }
    Ok(Block::Table { rows })
}
