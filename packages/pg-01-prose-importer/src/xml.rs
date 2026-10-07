// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{Error, ImportOptions, Result};
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::collections::BTreeMap;
use std::io::{self, BufReader, Read};

pub(crate) const FB: &str = "http://www.gribuser.ru/xml/fictionbook/2.0";
pub(crate) const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub(crate) const STRICT_WORD: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Name {
    pub ns: String,
    pub local: String,
}
impl Name {
    pub fn fb(&self, local: &str) -> bool {
        self.ns == FB && self.local == local
    }
    pub fn word(&self, local: &str) -> bool {
        matches!(self.ns.as_str(), WORD | STRICT_WORD) && self.local == local
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub name: Name,
    pub attrs: BTreeMap<(String, String), String>,
}
impl Element {
    pub fn attr(&self, key: &str) -> Option<&str> {
        let namespace = if self.name.fb("a") && key == "href" {
            "http://www.w3.org/1999/xlink"
        } else if self.name.word("hyperlink") && key == "id" {
            if self.name.ns == STRICT_WORD {
                "http://purl.oclc.org/ooxml/officeDocument/relationships"
            } else {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            }
        } else if matches!(self.name.ns.as_str(), WORD | STRICT_WORD) {
            &self.name.ns
        } else {
            ""
        };
        self.attrs
            .get(&(namespace.to_owned(), key.to_owned()))
            .map(String::as_str)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Token {
    Start(Element),
    End(Name),
    Text(String),
    Eof,
}
#[derive(Clone, Debug)]
pub(crate) enum Child {
    Node(Node),
    Text(String),
}
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub element: Element,
    pub children: Vec<Child>,
}
impl Node {
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|child| match child {
            Child::Node(node) => Some(node),
            _ => None,
        })
    }
    pub fn child_word(&self, name: &str) -> Option<&Node> {
        self.nodes().find(|node| node.element.name.word(name))
    }
    pub fn text(&self) -> String {
        let mut text = String::new();
        for child in &self.children {
            match child {
                Child::Text(s) => text.push_str(s),
                Child::Node(n) => text.push_str(&n.text()),
            }
        }
        text
    }
}

pub(crate) struct Limited<R> {
    reader: R,
    remaining: u64,
    token_remaining: Option<usize>,
}
impl<R> Limited<R> {
    pub fn new(reader: R, remaining: u64) -> Self {
        Self {
            reader,
            remaining,
            token_remaining: None,
        }
    }
}
impl<R: Read> Read for Limited<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut probe = [0];
            return if self.reader.read(&mut probe)? == 0 {
                Ok(0)
            } else {
                Err(io::Error::other("input byte limit exceeded"))
            };
        }
        if self.token_remaining == Some(0) {
            return Err(io::Error::other("XML token byte limit exceeded"));
        }
        let count = buffer
            .len()
            .min(self.remaining.min(usize::MAX as u64) as usize)
            .min(self.token_remaining.unwrap_or(usize::MAX));
        let read = self.reader.read(&mut buffer[..count])?;
        self.remaining -= read as u64;
        if let Some(remaining) = &mut self.token_remaining {
            *remaining -= read;
        }
        Ok(read)
    }
}

pub(crate) struct Xml<R: Read> {
    reader: NsReader<BufReader<Limited<R>>>,
    buffer: Vec<u8>,
    depth: usize,
    max_depth: usize,
    max_token_bytes: usize,
}
fn namespace(result: ResolveResult<'_>) -> Result<String> {
    match result {
        ResolveResult::Bound(ns) => Ok(std::str::from_utf8(ns.as_ref())
            .map_err(|_| Error::Invalid("namespace is not UTF-8"))?
            .into()),
        ResolveResult::Unbound => Ok(String::new()),
        ResolveResult::Unknown(_) => Err(Error::Invalid("unbound XML prefix")),
    }
}

impl<R: Read> Xml<R> {
    pub fn new(reader: R, options: &ImportOptions, limit: u64) -> Self {
        let mut reader = NsReader::from_reader(BufReader::new(Limited::new(reader, limit)));
        reader.config_mut().expand_empty_elements = true;
        Self {
            reader,
            buffer: Vec::new(),
            depth: 0,
            max_depth: options.max_depth,
            max_token_bytes: options.max_block_bytes.max(options.max_metadata_bytes),
        }
    }

    pub fn next(&mut self) -> Result<Token> {
        loop {
            self.buffer.clear();
            let buffered = self.reader.get_mut().buffer().len();
            self.reader.get_mut().get_mut().token_remaining = Some(
                self.max_token_bytes
                    .saturating_add(8192)
                    .saturating_sub(buffered),
            );
            let event = self.reader.read_event_into(&mut self.buffer)?;
            if !self.reader.decoder().encoding().is_ascii_compatible() {
                return Err(Error::Invalid(
                    "XML encoding must be ASCII-compatible, such as UTF-8 or Windows-1251",
                ));
            }
            match event {
                Event::Start(start) => {
                    self.depth += 1;
                    if self.depth > self.max_depth {
                        return Err(Error::Limit("XML depth"));
                    }
                    let (ns, local) = self.reader.resolve_element(start.name());
                    let name = Name {
                        ns: namespace(ns)?,
                        local: self
                            .reader
                            .decoder()
                            .decode(local.as_ref())
                            .map_err(|_| Error::Invalid("invalid XML name encoding"))?
                            .into_owned(),
                    };
                    let mut attrs = BTreeMap::new();
                    for attr in start.attributes() {
                        let attr = attr.map_err(|_| Error::Invalid("invalid XML attribute"))?;
                        if attr.key.as_ref() == b"xmlns" || attr.key.as_ref().starts_with(b"xmlns:")
                        {
                            continue;
                        }
                        let (ns, local) = self.reader.resolve_attribute(attr.key);
                        let namespace = namespace(ns)?;
                        let key = self
                            .reader
                            .decoder()
                            .decode(local.as_ref())
                            .map_err(|_| Error::Invalid("invalid XML attribute encoding"))?
                            .into_owned();
                        let value = attr
                            .decode_and_unescape_value(self.reader.decoder())?
                            .into_owned();
                        check_text(&value)?;
                        if attrs.insert((namespace, key), value).is_some() {
                            return Err(Error::Invalid("ambiguous XML attributes"));
                        }
                    }
                    return Ok(Token::Start(Element { name, attrs }));
                }
                Event::End(end) => {
                    if self.depth == 0 {
                        return Err(Error::Invalid("unexpected XML closing tag"));
                    }
                    self.depth -= 1;
                    let (ns, local) = self.reader.resolve_element(end.name());
                    return Ok(Token::End(Name {
                        ns: namespace(ns)?,
                        local: self
                            .reader
                            .decoder()
                            .decode(local.as_ref())
                            .map_err(|_| Error::Invalid("invalid XML name encoding"))?
                            .into_owned(),
                    }));
                }
                Event::Text(text) => {
                    let text = text
                        .xml10_content()
                        .map_err(|_| Error::Invalid("invalid XML text encoding"))?
                        .into_owned();
                    check_text(&text)?;
                    return Ok(Token::Text(text));
                }
                Event::CData(text) => {
                    let text = text
                        .xml10_content()
                        .map_err(|_| Error::Invalid("invalid XML CDATA encoding"))?
                        .into_owned();
                    check_text(&text)?;
                    return Ok(Token::Text(text));
                }
                Event::GeneralRef(reference) => {
                    let text = if let Some(character) = reference.resolve_char_ref()? {
                        character.to_string()
                    } else {
                        match reference
                            .decode()
                            .map_err(|_| Error::Invalid("invalid XML reference"))?
                            .as_ref()
                        {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            _ => return Err(Error::Invalid("undeclared XML entity")),
                        }
                        .into()
                    };
                    check_text(&text)?;
                    return Ok(Token::Text(text));
                }
                Event::DocType(_) => {
                    return Err(Error::Invalid(
                        "DOCTYPE and custom entities are not accepted",
                    ))
                }
                Event::Eof => {
                    if self.depth != 0 {
                        return Err(Error::Invalid("truncated XML"));
                    }
                    return Ok(Token::Eof);
                }
                _ => {}
            }
        }
    }

    pub fn tree(&mut self, element: Element, limit: usize) -> Result<(Node, usize)> {
        let mut bytes = 0;
        let node = self.tree_inner(element, limit, &mut bytes)?;
        Ok((node, bytes))
    }

    fn tree_inner(&mut self, element: Element, limit: usize, bytes: &mut usize) -> Result<Node> {
        *bytes = bytes.saturating_add(element.name.local.len() + element.name.ns.len());
        for (key, value) in &element.attrs {
            *bytes = bytes.saturating_add(key.0.len() + key.1.len() + value.len());
        }
        if *bytes > limit {
            return Err(Error::Limit("XML subtree bytes"));
        }
        let mut children = Vec::new();
        loop {
            match self.next()? {
                Token::Start(start) => {
                    children.push(Child::Node(self.tree_inner(start, limit, bytes)?))
                }
                Token::Text(text) => {
                    *bytes = bytes.saturating_add(text.len());
                    if *bytes > limit {
                        return Err(Error::Limit("XML subtree bytes"));
                    }
                    children.push(Child::Text(text));
                }
                Token::End(name) if name == element.name => break,
                _ => return Err(Error::Invalid("unexpected XML subtree end")),
            }
        }
        Ok(Node { element, children })
    }
}

fn check_text(text: &str) -> Result<()> {
    if text.chars().any(|c| {
        let n = c as u32;
        !matches!(n, 9 | 10 | 13)
            && !(0x20..=0xD7FF).contains(&n)
            && !(0xE000..=0xFFFD).contains(&n)
            && !(0x10000..=0x10FFFF).contains(&n)
    }) {
        return Err(Error::Invalid("illegal XML character"));
    }
    Ok(())
}
