// SPDX-License-Identifier: MIT OR Apache-2.0

//! Segmentation of `.yarn` sources into `title:` blocks, recursive block
//! parsing of their bodies, and assembly of the neutral graph.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::lex::{
    self, char_col, indent_width, split_indent, split_lines, split_speaker, CommandFrame, RawLine,
    ScanIssue,
};
use crate::model::{
    Clause, ClauseKeyword, Command, DialogueGraph, Edge, EdgeKind, Finding, FindingCode, Node,
    NodeKind, Title, Variable, VariableType,
};

/// Bounds applied while parsing. Every field must be greater than zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParseOptions {
    /// Largest accepted size of one input file, in bytes.
    pub max_input_bytes: usize,
    /// Largest total number of statement nodes across all inputs.
    pub max_nodes: usize,
    /// Largest total number of findings across all inputs.
    pub max_findings: usize,
    /// Deepest option/conditional nesting accepted before recursion stops.
    pub max_block_depth: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 8 * 1024 * 1024,
            max_nodes: 200_000,
            max_findings: 10_000,
            max_block_depth: 64,
        }
    }
}

impl ParseOptions {
    fn validate(self) -> Result<()> {
        if self.max_input_bytes > 0
            && self.max_nodes > 0
            && self.max_findings > 0
            && self.max_block_depth > 0
        {
            Ok(())
        } else {
            Err(Error::InvalidLimits)
        }
    }
}

/// One in-memory `.yarn` source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    /// Recorded verbatim in `sources` and in each finding's `file` field.
    pub name: String,
    /// File contents. A leading UTF-8 BOM is ignored.
    pub text: String,
}

impl Source {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }
}

/// Parses one source with the default bounds.
pub fn parse(name: &str, text: &str) -> Result<DialogueGraph> {
    parse_sources(&[Source::new(name, text)], &ParseOptions::default())
}

/// Parses several sources into one graph. Titles, line ids and variables share
/// a single namespace across all of them, matching how a Yarn project loads
/// every script in the project at once.
pub fn parse_sources(sources: &[Source], options: &ParseOptions) -> Result<DialogueGraph> {
    options.validate()?;
    if sources.is_empty() {
        return Err(Error::NoInput);
    }
    let names = sources.iter().map(|source| source.name.clone()).collect();
    let mut builder = Builder::new(names, options);
    for (index, source) in sources.iter().enumerate() {
        builder.parse_source(index, source)?;
    }
    builder.finish()
}

/// Commands that are part of the Yarn Spinner 2 vocabulary. Anything outside
/// this set still becomes a generic command node, plus an `unknown_command`
/// warning.
const KNOWN_COMMANDS: [&str; 9] = [
    "declare", "else", "elseif", "endif", "if", "jump", "set", "stop", "wait",
];

// ---------------------------------------------------------------------------
// classified lines
// ---------------------------------------------------------------------------

struct TextParts {
    speaker: Option<String>,
    text: String,
    markup: bool,
    line_id: Option<String>,
    tags: Vec<String>,
    condition: Option<String>,
}

struct TextLine {
    number: u32,
    indent: usize,
    /// Column of the first character of `parts.text`.
    col: u32,
    option: bool,
    parts: TextParts,
}

struct CommandLine {
    number: u32,
    indent: usize,
    col: u32,
    frame: CommandFrame,
}

enum BodyLine {
    Blank,
    /// A `///` doc-comment, with the slashes removed.
    Doc(String),
    Text(Box<TextLine>),
    Command(Box<CommandLine>),
}

impl BodyLine {
    fn position(&self) -> Option<(u32, u32)> {
        match self {
            Self::Text(line) => Some((line.number, line.col)),
            Self::Command(line) => Some((line.number, line.col)),
            Self::Blank | Self::Doc(_) => None,
        }
    }

    fn is_option_at(&self, indent: usize) -> bool {
        matches!(self, Self::Text(line) if line.option && line.indent == indent)
    }
}

// ---------------------------------------------------------------------------
// item tree
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Item {
    Node(usize),
    Choice(usize),
    Branch(usize),
}

#[derive(Clone, Debug)]
struct ChoiceOption {
    node: usize,
    body: Vec<Item>,
}

#[derive(Clone, Debug)]
struct ClauseBody {
    label: String,
    body: Vec<Item>,
}

/// Where a clause keyword came from and what it tested.
struct ClauseMeta {
    keyword: ClauseKeyword,
    expression: Option<String>,
    line: u32,
    col: u32,
}

#[derive(Debug)]
enum BlockEnd {
    Done,
    Dedent,
    Clause,
    EndIf,
}

#[derive(Clone, Debug)]
struct Flow {
    targets: Vec<usize>,
    kind: EdgeKind,
}

const NO_FLOW: Flow = Flow {
    targets: Vec::new(),
    kind: EdgeKind::Flow,
};

fn link_kind(item: &Item) -> EdgeKind {
    match item {
        Item::Choice(_) => EdgeKind::Option,
        Item::Node(_) | Item::Branch(_) => EdgeKind::Flow,
    }
}

// ---------------------------------------------------------------------------
// builder
// ---------------------------------------------------------------------------

struct Builder<'options> {
    options: &'options ParseOptions,
    graph: DialogueGraph,
    findings: Vec<(usize, Finding)>,
    dropped_findings: Option<usize>,
    scopes_in_order: Vec<String>,
    scope_by_title: HashMap<String, String>,
    scope_entry: HashMap<String, usize>,
    title_counts: HashMap<String, usize>,
    node_counters: HashMap<String, usize>,
    line_id_owner: HashMap<String, (String, u32, u32)>,
    declared: HashSet<String>,
    choice_groups: HashMap<usize, Vec<ChoiceOption>>,
    branches: HashMap<usize, Vec<ClauseBody>>,
    clause_meta: HashMap<usize, Vec<ClauseMeta>>,
    /// Parsed bodies awaiting edge emission, in scope order.
    bodies: Vec<Vec<Item>>,
    group_keys: usize,
    source_index: usize,
    scope: String,
    option_group: usize,
    pending_doc: Option<String>,
}

impl<'options> Builder<'options> {
    fn new(sources: Vec<String>, options: &'options ParseOptions) -> Self {
        Self {
            graph: DialogueGraph::new(sources),
            options,
            findings: Vec::new(),
            dropped_findings: None,
            scopes_in_order: Vec::new(),
            scope_by_title: HashMap::new(),
            scope_entry: HashMap::new(),
            title_counts: HashMap::new(),
            node_counters: HashMap::new(),
            line_id_owner: HashMap::new(),
            declared: HashSet::new(),
            choice_groups: HashMap::new(),
            branches: HashMap::new(),
            clause_meta: HashMap::new(),
            bodies: Vec::new(),
            group_keys: 0,
            source_index: 0,
            scope: String::new(),
            option_group: 0,
            pending_doc: None,
        }
    }

    // -- findings ----------------------------------------------------------

    fn finding(&mut self, code: FindingCode, line: u32, col: u32, message: impl Into<String>) {
        self.finding_in(self.source_index, code, line, col, message);
    }

    fn finding_in(
        &mut self,
        source: usize,
        code: FindingCode,
        line: u32,
        col: u32,
        message: impl Into<String>,
    ) {
        if self.findings.len() >= self.options.max_findings {
            self.dropped_findings.get_or_insert(source);
            return;
        }
        self.findings.push((
            source,
            Finding {
                file: self.graph.sources[source].clone(),
                line,
                col,
                code,
                severity: code.severity(),
                message: message.into(),
            },
        ));
    }

    // -- source segmentation ----------------------------------------------

    fn parse_source(&mut self, index: usize, source: &Source) -> Result<()> {
        self.source_index = index;
        let text = source.text.strip_prefix('\u{feff}').unwrap_or(&source.text);
        let lines = split_lines(text);

        let mut headers: Vec<(String, String, u32, u32)> = Vec::new();
        let mut header_at: Option<(u32, u32)> = None;
        let mut body: Vec<BodyLine> = Vec::new();
        let mut in_body = false;

        for raw in &lines {
            let (whitespace, rest) = split_indent(raw.text);
            let offset = whitespace.len();
            let content = rest.trim_end();
            let col = char_col(raw.text, offset);

            if in_body {
                match content {
                    "===" => {
                        self.close_node(
                            std::mem::take(&mut headers),
                            header_at,
                            std::mem::take(&mut body),
                        )?;
                        header_at = None;
                        in_body = false;
                    }
                    "---" => self.finding(
                        FindingCode::UnexpectedHeaderDelimiter,
                        raw.number,
                        col,
                        "`---` inside a node body; headers must come before the body",
                    ),
                    _ => body.push(self.classify(raw, content, offset, col)),
                }
                continue;
            }

            if content.is_empty() || content.starts_with("//") {
                continue;
            }
            match content {
                "---" => {
                    in_body = true;
                    body = Vec::new();
                    header_at.get_or_insert((raw.number, col));
                    continue;
                }
                "===" => {
                    if headers.is_empty() {
                        self.finding(
                            FindingCode::UnexpectedNodeEnd,
                            raw.number,
                            col,
                            "`===` with no open node",
                        );
                        continue;
                    }
                    let at = header_at.unwrap_or((raw.number, col));
                    self.finding(
                        FindingCode::MissingHeaderDelimiter,
                        at.0,
                        at.1,
                        "node headers are not followed by `---`",
                    );
                    self.close_node(std::mem::take(&mut headers), header_at, Vec::new())?;
                    header_at = None;
                    continue;
                }
                _ => {}
            }
            if let Some((key, value)) = split_header(content) {
                header_at.get_or_insert((raw.number, col));
                headers.push((key, value, raw.number, col));
                continue;
            }
            if content.starts_with("->") {
                self.finding(
                    FindingCode::OptionAfterNodeEnd,
                    raw.number,
                    col,
                    "option outside a node body; `===` already closed the node",
                );
            } else {
                self.finding(
                    FindingCode::ContentOutsideNode,
                    raw.number,
                    col,
                    "content outside a node body; expected a `key: value` header or `---`",
                );
            }
        }

        if in_body {
            self.close_node(headers, header_at, body)?;
        } else if let Some(at) = header_at {
            self.finding(
                FindingCode::MissingHeaderDelimiter,
                at.0,
                at.1,
                "node headers are not followed by `---`",
            );
            self.close_node(headers, header_at, Vec::new())?;
        }
        if self.dropped_findings.is_some() {
            return Err(Error::TooManyFindings {
                source: source.name.clone(),
                limit: self.options.max_findings,
            });
        }
        Ok(())
    }

    /// Classifies one body line. `content` has no leading indentation and no
    /// trailing whitespace; `offset` is its byte offset inside `raw.text`.
    fn classify(&mut self, raw: &RawLine<'_>, content: &str, offset: usize, col: u32) -> BodyLine {
        let indent = indent_width(&raw.text[..offset]);

        if content.is_empty() {
            return BodyLine::Blank;
        }
        if let Some(doc) = content.strip_prefix("///") {
            return BodyLine::Doc(doc.trim().to_string());
        }
        if content.starts_with("//") {
            return BodyLine::Blank;
        }
        if content.starts_with("<<") {
            let Some(frame) = lex::find_command(content, 0) else {
                self.finding(
                    FindingCode::UnterminatedCommand,
                    raw.number,
                    col,
                    "`<<` has no matching `>>`",
                );
                return BodyLine::Blank;
            };
            if !content[frame.end..].trim().is_empty() {
                self.finding(
                    FindingCode::TrailingContentAfterCommand,
                    raw.number,
                    char_col(raw.text, offset + frame.end),
                    "text after `>>` is ignored",
                );
            }
            return BodyLine::Command(Box::new(CommandLine {
                number: raw.number,
                indent,
                col,
                frame,
            }));
        }

        let option = content.starts_with("->");
        let skip = if option {
            let after = &content[2..];
            2 + (after.len() - after.trim_start().len())
        } else {
            0
        };
        let text_offset = offset + skip;
        let scan = lex::scan_line(&content[skip..]);
        for (issue_offset, issue) in &scan.issues {
            let (code, message) = match issue {
                ScanIssue::MalformedHashtag => (
                    FindingCode::MalformedHashtag,
                    "hashtag region holds a token that does not start with `#`",
                ),
                ScanIssue::EmptyLineId => (FindingCode::EmptyLineId, "`#line:` has no id"),
                ScanIssue::DuplicateLineId => (
                    FindingCode::DuplicateLineId,
                    "line carries more than one `#line:` hashtag",
                ),
            };
            self.finding(
                code,
                raw.number,
                char_col(raw.text, text_offset + issue_offset),
                message,
            );
        }

        let mut text = scan.text;
        let mut condition = None;
        if option {
            if let Some(frame) = lex::find_command(&text, 0) {
                if frame.start > 0
                    && text[frame.end..].trim().is_empty()
                    && frame.name.eq_ignore_ascii_case("if")
                {
                    condition = Some(frame.args);
                    text = text[..frame.start].trim_end().to_string();
                }
            }
        }
        let before_speaker = text.len();
        let (speaker, body) = split_speaker(&text);
        let speaker_shift = if speaker.is_some() {
            before_speaker - body.len()
        } else {
            0
        };
        let markup = lex::has_markup(&body);
        if body.is_empty() && scan.line_id.is_none() && scan.tags.is_empty() && !option {
            return BodyLine::Blank;
        }
        BodyLine::Text(Box::new(TextLine {
            number: raw.number,
            indent,
            col: char_col(raw.text, text_offset + speaker_shift),
            option,
            parts: TextParts {
                speaker,
                text: body,
                markup,
                line_id: scan.line_id,
                tags: scan.tags,
                condition,
            },
        }))
    }

    /// Closes one `title:` block: records headers, parses the body, keeps the
    /// item tree for the later edge pass.
    fn close_node(
        &mut self,
        headers: Vec<(String, String, u32, u32)>,
        header_at: Option<(u32, u32)>,
        body: Vec<BodyLine>,
    ) -> Result<()> {
        let mut title: Option<(String, u32, u32)> = None;
        let mut tags: Vec<String> = Vec::new();
        let mut meta: Map<String, Value> = Map::new();
        let mut seen: HashSet<String> = HashSet::new();

        for (key, value, line, col) in headers {
            let lower = key.to_ascii_lowercase();
            if !seen.insert(lower.clone()) {
                self.finding(
                    FindingCode::DuplicateHeaderKey,
                    line,
                    col,
                    format!("header `{key}` appears more than once; the first value is kept"),
                );
                continue;
            }
            match lower.as_str() {
                "title" => title = Some((value, line, col)),
                "tags" => tags = split_tags(&value),
                _ => {
                    meta.insert(key, Value::String(value));
                }
            }
        }

        let explicit = title
            .as_ref()
            .is_some_and(|(value, _, _)| !value.trim().is_empty());
        let written = if explicit {
            title.as_ref().expect("checked").0.trim().to_string()
        } else {
            let at = title
                .as_ref()
                .map(|(_, line, col)| (*line, *col))
                .or(header_at)
                .unwrap_or((1, 1));
            self.finding(
                FindingCode::MissingNodeTitle,
                at.0,
                at.1,
                "node has no usable `title:` header; an `untitled` scope was assigned",
            );
            "untitled".to_string()
        };
        let at = title
            .as_ref()
            .map(|(_, line, col)| (*line, *col))
            .or(header_at)
            .unwrap_or((1, 1));

        let occurrence = {
            let count = self.title_counts.entry(written.clone()).or_insert(0);
            *count += 1;
            *count
        };
        let scope = if occurrence == 1 {
            written.clone()
        } else {
            if explicit {
                self.finding(
                    FindingCode::DuplicateNodeTitle,
                    at.0,
                    at.1,
                    format!(
                        "node title `{written}` is already defined; this block becomes `{written}#{occurrence}`"
                    ),
                );
            }
            format!("{written}#{occurrence}")
        };

        self.scope = scope.clone();
        self.option_group = 0;
        self.node_counters.insert(scope.clone(), 0);
        self.scopes_in_order.push(scope.clone());
        self.scope_by_title
            .entry(written.clone())
            .or_insert_with(|| scope.clone());
        let title_index = self.graph.titles.len();
        self.graph.titles.push(Title {
            id: scope.clone(),
            title: written,
            source: self.source_index,
            tags,
            meta,
            entry: None,
            line: at.0,
            col: at.1,
        });

        let mut cursor = 0;
        let first_index = self.graph.nodes.len();
        let (items, _) = self.parse_block(&body, &mut cursor, 0, false, None, None, 0)?;
        if self.graph.nodes.len() > first_index {
            self.scope_entry.insert(scope.clone(), first_index);
            self.graph.titles[title_index].entry = Some(format!("{scope}:1"));
        }
        self.bodies.push(items);
        Ok(())
    }

    // -- block parsing -----------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn parse_block(
        &mut self,
        lines: &[BodyLine],
        cursor: &mut usize,
        min_indent: usize,
        in_conditional: bool,
        parent: Option<String>,
        clause: Option<String>,
        depth: usize,
    ) -> Result<(Vec<Item>, BlockEnd)> {
        if depth > self.options.max_block_depth {
            if let Some((line, col)) = lines.get(*cursor).and_then(BodyLine::position) {
                self.finding(
                    FindingCode::NestingTooDeep,
                    line,
                    col,
                    format!(
                        "nesting exceeds {} levels; the rest of this node was skipped",
                        self.options.max_block_depth
                    ),
                );
            }
            *cursor = lines.len();
            return Ok((Vec::new(), BlockEnd::Done));
        }

        let mut items: Vec<Item> = Vec::new();
        while *cursor < lines.len() {
            match &lines[*cursor] {
                BodyLine::Blank => {
                    self.pending_doc = None;
                    *cursor += 1;
                }
                BodyLine::Doc(text) => {
                    self.push_doc(text);
                    *cursor += 1;
                }
                BodyLine::Command(command) => {
                    if command.indent < min_indent {
                        return Ok((items, BlockEnd::Dedent));
                    }
                    let name = command.frame.name.to_ascii_lowercase();
                    let (number, col, indent) = (command.number, command.col, command.indent);
                    match name.as_str() {
                        "elseif" | "else" if in_conditional => {
                            return Ok((items, BlockEnd::Clause));
                        }
                        "endif" if in_conditional => return Ok((items, BlockEnd::EndIf)),
                        "elseif" | "else" | "endif" => {
                            self.pending_doc = None;
                            self.finding(
                                FindingCode::UnbalancedConditional,
                                number,
                                col,
                                format!("`<<{name}>>` has no matching `<<if>>`"),
                            );
                            *cursor += 1;
                        }
                        "if" => {
                            let frame = command.frame.clone();
                            self.pending_doc = None;
                            *cursor += 1;
                            let item = self.parse_conditional(
                                &frame,
                                lines,
                                cursor,
                                indent,
                                &parent,
                                clause.clone(),
                                depth,
                                number,
                                col,
                            )?;
                            items.push(item);
                        }
                        _ => {
                            let frame = command.frame.clone();
                            let doc = self.pending_doc.take();
                            *cursor += 1;
                            let index = self.push_command_node(
                                &frame,
                                number,
                                col,
                                parent.clone(),
                                clause.clone(),
                                doc,
                            )?;
                            items.push(Item::Node(index));
                        }
                    }
                }
                BodyLine::Text(text) => {
                    if text.indent < min_indent {
                        return Ok((items, BlockEnd::Dedent));
                    }
                    self.pending_doc = None;
                    if text.option {
                        let indent = text.indent;
                        let item =
                            self.parse_choice_group(lines, cursor, indent, &parent, depth)?;
                        items.push(item);
                    } else {
                        let index =
                            self.push_text_node(text, None, parent.clone(), clause.clone())?;
                        items.push(Item::Node(index));
                        *cursor += 1;
                    }
                }
            }
        }
        Ok((items, BlockEnd::Done))
    }

    fn push_doc(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        match self.pending_doc.take() {
            Some(previous) => self.pending_doc = Some(format!("{previous} {text}")),
            None => self.pending_doc = Some(text.to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_conditional(
        &mut self,
        frame: &CommandFrame,
        lines: &[BodyLine],
        cursor: &mut usize,
        indent: usize,
        parent: &Option<String>,
        clause: Option<String>,
        depth: usize,
        number: u32,
        col: u32,
    ) -> Result<Item> {
        let expression = self.command_expression(frame, number, col, "if");
        let node_index =
            self.push_conditional_node(number, col, parent.clone(), clause, expression.clone())?;
        let conditional_id = self.graph.nodes[node_index].id.clone();
        let mut clauses = vec![ClauseBody {
            label: "if".to_string(),
            body: Vec::new(),
        }];
        let mut meta = vec![ClauseMeta {
            keyword: ClauseKeyword::If,
            expression,
            line: number,
            col,
        }];
        let mut elseif_count = 0usize;
        let mut saw_else = false;

        loop {
            let mut inner = *cursor;
            let label = clauses.last().map(|clause| clause.label.clone());
            let (body, end) = self.parse_block(
                lines,
                &mut inner,
                indent,
                true,
                Some(conditional_id.clone()),
                label,
                depth + 1,
            )?;
            *cursor = inner;
            // `extend`, not assignment: a clause keyword rejected after
            // `<<else>>` re-parses into the clause that is still open, and its
            // statements must join the ones already collected there.
            if let Some(last) = clauses.last_mut() {
                last.body.extend(body);
            }
            match end {
                BlockEnd::EndIf => {
                    *cursor += 1;
                    break;
                }
                BlockEnd::Clause => {
                    let Some(BodyLine::Command(command)) = lines.get(*cursor) else {
                        break;
                    };
                    let keyword_frame = command.frame.clone();
                    let name = keyword_frame.name.to_ascii_lowercase();
                    let (clause_line, clause_col) = (command.number, command.col);
                    *cursor += 1;
                    if saw_else {
                        self.finding(
                            FindingCode::UnbalancedConditional,
                            clause_line,
                            clause_col,
                            format!("`<<{name}>>` after `<<else>>`"),
                        );
                        continue;
                    }
                    if name == "else" {
                        saw_else = true;
                        if !keyword_frame.args.is_empty() {
                            self.finding(
                                FindingCode::TrailingContentAfterCommand,
                                clause_line,
                                clause_col,
                                "`<<else>>` takes no condition",
                            );
                        }
                        clauses.push(ClauseBody {
                            label: "else".to_string(),
                            body: Vec::new(),
                        });
                        meta.push(ClauseMeta {
                            keyword: ClauseKeyword::Else,
                            expression: None,
                            line: clause_line,
                            col: clause_col,
                        });
                    } else {
                        elseif_count += 1;
                        let expression = self.command_expression(
                            &keyword_frame,
                            clause_line,
                            clause_col,
                            "elseif",
                        );
                        clauses.push(ClauseBody {
                            label: format!("elseif {elseif_count}"),
                            body: Vec::new(),
                        });
                        meta.push(ClauseMeta {
                            keyword: ClauseKeyword::ElseIf,
                            expression,
                            line: clause_line,
                            col: clause_col,
                        });
                    }
                }
                BlockEnd::Dedent | BlockEnd::Done => {
                    self.finding(
                        FindingCode::UnbalancedConditional,
                        number,
                        col,
                        "`<<if>>` has no matching `<<endif>>`",
                    );
                    break;
                }
            }
        }

        self.branches.insert(node_index, clauses);
        self.clause_meta.insert(node_index, meta);
        Ok(Item::Branch(node_index))
    }

    fn command_expression(
        &mut self,
        frame: &CommandFrame,
        line: u32,
        col: u32,
        keyword: &str,
    ) -> Option<String> {
        if frame.args.is_empty() {
            self.finding(
                FindingCode::BadCommandSyntax,
                line,
                col,
                format!("`<<{keyword}>>` has no condition"),
            );
            None
        } else {
            Some(frame.args.clone())
        }
    }

    fn parse_choice_group(
        &mut self,
        lines: &[BodyLine],
        cursor: &mut usize,
        indent: usize,
        parent: &Option<String>,
        depth: usize,
    ) -> Result<Item> {
        self.option_group += 1;
        let group = self.option_group;
        let mut options: Vec<ChoiceOption> = Vec::new();

        while let BodyLine::Text(text) = &lines[*cursor] {
            let index = self.push_text_node(text, Some(group), parent.clone(), None)?;
            let option_id = self.graph.nodes[index].id.clone();
            *cursor += 1;
            let mut inner = *cursor;
            let (body, _) = self.parse_block(
                lines,
                &mut inner,
                indent + 1,
                false,
                Some(option_id),
                None,
                depth + 1,
            )?;
            *cursor = inner;
            options.push(ChoiceOption { node: index, body });
            if !lines
                .get(*cursor)
                .is_some_and(|line| line.is_option_at(indent))
            {
                break;
            }
        }

        self.group_keys += 1;
        let key = self.group_keys;
        self.choice_groups.insert(key, options);
        Ok(Item::Choice(key))
    }

    // -- node construction -------------------------------------------------

    fn next_id(&mut self) -> Result<String> {
        if self.graph.nodes.len() >= self.options.max_nodes {
            return Err(Error::TooManyNodes {
                source: self.graph.sources[self.source_index].clone(),
                limit: self.options.max_nodes,
            });
        }
        let counter = self.node_counters.entry(self.scope.clone()).or_insert(0);
        *counter += 1;
        Ok(format!("{}:{}", self.scope, counter))
    }

    fn push_node(&mut self, node: Node) -> usize {
        self.graph.nodes.push(node);
        self.graph.nodes.len() - 1
    }

    fn push_text_node(
        &mut self,
        line: &TextLine,
        group: Option<usize>,
        parent: Option<String>,
        clause: Option<String>,
    ) -> Result<usize> {
        let id = self.next_id()?;
        if let Some(line_id) = &line.parts.line_id {
            match self.line_id_owner.get(line_id) {
                Some((file, first_line, first_col)) => self.finding(
                    FindingCode::DuplicateLineId,
                    line.number,
                    line.col,
                    format!(
                        "`#line:{line_id}` is already used by {file} line {first_line} column {first_col}"
                    ),
                ),
                None => {
                    self.line_id_owner.insert(
                        line_id.clone(),
                        (self.graph.sources[self.source_index].clone(), line.number, line.col),
                    );
                }
            }
        }
        let node = Node {
            id,
            source: self.source_index,
            title: self.scope.clone(),
            kind: if line.option {
                NodeKind::Options
            } else {
                NodeKind::Line
            },
            speaker: line.parts.speaker.clone(),
            text: Some(line.parts.text.clone()),
            line_id: line.parts.line_id.clone(),
            tags: line.parts.tags.clone(),
            markup: line.parts.markup,
            condition: line.parts.condition.clone(),
            option_group: group,
            parent,
            clause,
            command: None,
            line: line.number,
            col: line.col,
        };
        Ok(self.push_node(node))
    }

    fn push_conditional_node(
        &mut self,
        line: u32,
        col: u32,
        parent: Option<String>,
        clause: Option<String>,
        expression: Option<String>,
    ) -> Result<usize> {
        let id = self.next_id()?;
        let node = Node {
            id,
            source: self.source_index,
            title: self.scope.clone(),
            kind: NodeKind::Conditional,
            speaker: None,
            text: None,
            line_id: None,
            tags: Vec::new(),
            markup: false,
            condition: None,
            option_group: None,
            parent,
            clause,
            command: Some(Command {
                name: "if".to_string(),
                args: expression.clone(),
                variable: None,
                operator: None,
                expression,
                target: None,
                clauses: Vec::new(),
            }),
            line,
            col,
        };
        Ok(self.push_node(node))
    }

    fn push_command_node(
        &mut self,
        frame: &CommandFrame,
        line: u32,
        col: u32,
        parent: Option<String>,
        clause: Option<String>,
        doc: Option<String>,
    ) -> Result<usize> {
        let lower = frame.name.to_ascii_lowercase();
        let mut command = Command {
            name: frame.name.clone(),
            args: (!frame.args.is_empty()).then(|| frame.args.clone()),
            variable: None,
            operator: None,
            expression: None,
            target: None,
            clauses: Vec::new(),
        };
        let kind = match lower.as_str() {
            "jump" => {
                if frame.args.is_empty() {
                    self.finding(
                        FindingCode::BadCommandSyntax,
                        line,
                        col,
                        "`<<jump>>` has no target node",
                    );
                } else {
                    command.target = Some(frame.args.clone());
                }
                NodeKind::Jump
            }
            "set" | "declare" => {
                let declare = lower == "declare";
                self.parse_assignment(
                    frame,
                    line,
                    col,
                    &mut command,
                    declare,
                    doc.filter(|text| !text.is_empty()),
                );
                NodeKind::Command
            }
            "stop" | "wait" => NodeKind::Command,
            _ => {
                if !KNOWN_COMMANDS.contains(&lower.as_str()) {
                    self.finding(
                        FindingCode::UnknownCommand,
                        line,
                        col,
                        format!(
                            "`<<{}>>` is not a Yarn Spinner 2 command; kept as a generic command node",
                            frame.name
                        ),
                    );
                }
                NodeKind::Command
            }
        };
        let id = self.next_id()?;
        let node = Node {
            id,
            source: self.source_index,
            title: self.scope.clone(),
            kind,
            speaker: None,
            text: None,
            line_id: None,
            tags: Vec::new(),
            markup: false,
            condition: None,
            option_group: None,
            parent,
            clause,
            command: Some(command),
            line,
            col,
        };
        Ok(self.push_node(node))
    }

    fn parse_assignment(
        &mut self,
        frame: &CommandFrame,
        line: u32,
        col: u32,
        command: &mut Command,
        declare: bool,
        doc: Option<String>,
    ) {
        let keyword = if declare { "declare" } else { "set" };
        let Some((start, end, operator)) = find_assignment(&frame.args) else {
            self.finding(
                FindingCode::BadCommandSyntax,
                line,
                col,
                format!("`<<{keyword}>>` has no assignment operator"),
            );
            return;
        };
        let Some(name) = frame.args[..start]
            .trim()
            .strip_prefix('$')
            .filter(|name| !name.is_empty())
        else {
            self.finding(
                FindingCode::BadCommandSyntax,
                line,
                col,
                format!("`<<{keyword}>>` target is not a `$variable`"),
            );
            return;
        };
        let value = frame.args[end..].trim().to_string();
        command.variable = Some(name.to_string());
        command.operator = Some(operator.to_string());
        command.expression = Some(value.clone());
        if declare {
            self.declare_variable(name, &value, line, col, doc);
        }
    }

    fn declare_variable(
        &mut self,
        name: &str,
        raw_value: &str,
        line: u32,
        col: u32,
        doc: Option<String>,
    ) {
        let (value_text, declared_as) = split_declared_as(raw_value);
        let (inferred, value) = infer_literal(&value_text);
        let declared_type =
            self.resolve_declared_type(name, declared_as.as_deref(), inferred, line, col);
        if !self.declared.insert(name.to_string()) {
            self.finding(
                FindingCode::DuplicateVariable,
                line,
                col,
                format!("`${name}` is declared more than once; the first declaration is kept"),
            );
            return;
        }
        self.graph.variables.push(Variable {
            name: name.to_string(),
            declared_type,
            declared_as,
            value,
            description: doc,
            source: self.source_index,
            title: self.scope.clone(),
            line,
            col,
        });
    }

    /// Yarn fixes a variable's type at its declaration. An explicit `as`
    /// suffix wins; otherwise the type comes from the literal.
    fn resolve_declared_type(
        &mut self,
        name: &str,
        declared_as: Option<&str>,
        inferred: VariableType,
        line: u32,
        col: u32,
    ) -> VariableType {
        let Some(written) = declared_as else {
            if inferred == VariableType::Unknown {
                self.finding(
                    FindingCode::InvalidVariableType,
                    line,
                    col,
                    format!("the initial value of `${name}` is not a literal"),
                );
            }
            return inferred;
        };
        let written = written.to_ascii_lowercase();
        let agreed = match written.as_str() {
            "bool" | "boolean" => Some(matches!(inferred, VariableType::Bool)),
            "string" => Some(matches!(inferred, VariableType::String)),
            "number" | "int" | "integer" | "float" | "double" => {
                Some(matches!(inferred, VariableType::Int | VariableType::Float))
            }
            _ => None,
        };
        let Some(agreed) = agreed else {
            self.finding(
                FindingCode::InvalidVariableType,
                line,
                col,
                format!("`as {written}` is not `bool`, `string`, `int`, `float` or `number`"),
            );
            return inferred;
        };
        if !agreed {
            self.finding(
                FindingCode::InvalidVariableType,
                line,
                col,
                format!("`as {written}` does not match the initial value of `${name}`"),
            );
        }
        match written.as_str() {
            "bool" | "boolean" => VariableType::Bool,
            "string" => VariableType::String,
            "int" | "integer" if !matches!(inferred, VariableType::Float) => VariableType::Int,
            _ => inferred,
        }
    }

    // -- edges -------------------------------------------------------------

    fn push_edge(&mut self, kind: EdgeKind, source: usize, target: usize, label: Option<String>) {
        let index = self.graph.edges.len();
        let (from, to) = (
            self.graph.nodes[source].id.clone(),
            self.graph.nodes[target].id.clone(),
        );
        self.graph.edges.push(Edge {
            id: format!("edge-{}", index + 1),
            kind,
            source: from,
            target: to,
            index,
            label,
        });
    }

    fn entries(&self, item: &Item) -> Vec<usize> {
        match item {
            Item::Node(index) => vec![*index],
            Item::Choice(key) => self
                .choice_groups
                .get(key)
                .map(|options| options.iter().map(|option| option.node).collect())
                .unwrap_or_default(),
            Item::Branch(index) => vec![*index],
        }
    }

    fn flow_after(&self, items: &[Item], index: usize, following: &Flow) -> Flow {
        match items.get(index + 1) {
            Some(next) => Flow {
                targets: self.entries(next),
                kind: link_kind(next),
            },
            None => following.clone(),
        }
    }

    fn emit_block(
        &mut self,
        items: &[Item],
        incoming: &[usize],
        incoming_kind: Option<EdgeKind>,
        incoming_label: Option<&str>,
        following: &Flow,
    ) {
        for (index, item) in items.iter().enumerate() {
            let next = self.flow_after(items, index, following);
            if index == 0 && !incoming.is_empty() {
                let kind = incoming_kind.unwrap_or_else(|| link_kind(item));
                let targets = self.entries(item);
                for source in incoming {
                    for target in &targets {
                        self.push_edge(kind, *source, *target, incoming_label.map(str::to_string));
                    }
                }
            }
            match item {
                Item::Node(node) => self.emit_statement(*node, &next),
                Item::Choice(key) => {
                    let options = self.choice_groups.get(key).cloned().unwrap_or_default();
                    for option in &options {
                        if option.body.is_empty() {
                            for target in &next.targets {
                                self.push_edge(next.kind, option.node, *target, None);
                            }
                        } else {
                            self.emit_block(&option.body, &[option.node], None, None, &next);
                        }
                    }
                }
                Item::Branch(node) => {
                    let clauses = self.branches.get(node).cloned().unwrap_or_default();
                    for clause in &clauses {
                        if clause.body.is_empty() {
                            for target in &next.targets {
                                self.push_edge(
                                    EdgeKind::Branch,
                                    *node,
                                    *target,
                                    Some(clause.label.clone()),
                                );
                            }
                        } else {
                            self.emit_block(
                                &clause.body,
                                &[*node],
                                Some(EdgeKind::Branch),
                                Some(&clause.label),
                                &next,
                            );
                        }
                    }
                }
            }
        }
    }

    fn emit_statement(&mut self, node: usize, next: &Flow) {
        if is_stop(&self.graph.nodes[node]) {
            return;
        }
        if self.graph.nodes[node].kind != NodeKind::Jump {
            for target in &next.targets {
                self.push_edge(next.kind, node, *target, None);
            }
            return;
        }
        let Some(title) = self.graph.nodes[node]
            .command
            .as_ref()
            .and_then(|command| command.target.clone())
        else {
            return;
        };
        let resolved = self
            .scope_by_title
            .get(&title)
            .and_then(|scope| self.scope_entry.get(scope))
            .copied();
        match resolved {
            Some(target) => self.push_edge(EdgeKind::Jump, node, target, None),
            None => {
                let source = self.graph.nodes[node].source;
                let line = self.graph.nodes[node].line;
                let col = self.graph.nodes[node].col;
                self.finding_in(
                    source,
                    FindingCode::MissingJumpTarget,
                    line,
                    col,
                    format!("`<<jump {title}>>` names a node that does not exist"),
                );
            }
        }
    }

    // -- assembly ----------------------------------------------------------

    fn finish(mut self) -> Result<DialogueGraph> {
        let bodies = std::mem::take(&mut self.bodies);
        for body in &bodies {
            self.emit_block(body, &[], None, None, &NO_FLOW);
        }

        for (node_index, clauses) in std::mem::take(&mut self.branches) {
            let meta = self.clause_meta.remove(&node_index).unwrap_or_default();
            let built: Vec<Clause> = clauses
                .into_iter()
                .zip(meta)
                .map(|(clause, meta)| Clause {
                    keyword: meta.keyword,
                    label: clause.label,
                    expression: meta.expression,
                    body: clause
                        .body
                        .iter()
                        .flat_map(|item| self.entries(item))
                        .map(|index| self.graph.nodes[index].id.clone())
                        .collect(),
                    line: meta.line,
                    col: meta.col,
                })
                .collect();
            if let Some(command) = self.graph.nodes[node_index].command.as_mut() {
                command.clauses = built;
            }
        }

        self.findings.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| {
                    (left.1.line, left.1.col, left.1.code).cmp(&(
                        right.1.line,
                        right.1.col,
                        right.1.code,
                    ))
                })
                .then_with(|| left.1.message.cmp(&right.1.message))
        });
        self.graph.errors = self
            .findings
            .into_iter()
            .map(|(_, finding)| finding)
            .collect();

        // A block titled `Start` wins, but only when it has a body; an empty
        // `Start` should not hide the first node a runtime could actually enter.
        let start = self
            .graph
            .titles
            .iter()
            .filter(|title| title.entry.is_some())
            .find(|title| title.title == "Start")
            .or_else(|| self.graph.titles.iter().find(|title| title.entry.is_some()))
            .and_then(|title| title.entry.clone());
        self.graph.start_node = start;

        if let Some(index) = self.dropped_findings {
            return Err(Error::TooManyFindings {
                source: self.graph.sources[index].clone(),
                limit: self.options.max_findings,
            });
        }
        Ok(self.graph)
    }
}

fn is_stop(node: &Node) -> bool {
    node.command
        .as_ref()
        .is_some_and(|command| command.name.eq_ignore_ascii_case("stop"))
}

fn split_header(content: &str) -> Option<(String, String)> {
    let index = content.find(':')?;
    let key = &content[..index];
    let identifier = !key.is_empty()
        && !key.chars().next().is_some_and(|c| c.is_ascii_digit())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !identifier {
        return None;
    }
    Some((key.to_string(), content[index + 1..].trim().to_string()))
}

fn split_tags(value: &str) -> Vec<String> {
    value
        .split([',', ' ', '\t'])
        .map(|token| token.trim().trim_start_matches('#').trim())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Locates the first `=`, `+=`, `-=`, `*=`, `/=` or standalone `to`.
fn find_assignment(args: &str) -> Option<(usize, usize, &'static str)> {
    let bytes = args.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => index = skip_quoted(bytes, index),
            b'=' => {
                if index > 0 && matches!(bytes[index - 1], b'+' | b'-' | b'*' | b'/') {
                    return Some((index - 1, index + 1, compound(bytes[index - 1])));
                }
                return Some((index, index + 1, "="));
            }
            b't' if index > 0
                && bytes[index - 1].is_ascii_whitespace()
                && word_is(args, index, "to") =>
            {
                return Some((index, index + 2, "to"));
            }
            _ => index += 1,
        }
    }
    None
}

fn compound(byte: u8) -> &'static str {
    match byte {
        b'+' => "+=",
        b'-' => "-=",
        b'*' => "*=",
        _ => "/=",
    }
}

fn word_is(text: &str, index: usize, word: &str) -> bool {
    let end = index + word.len();
    if !text.is_char_boundary(end) || text[index..end] != *word {
        return false;
    }
    text[end..]
        .chars()
        .next()
        .is_none_or(|c| c.is_ascii_whitespace())
}

fn at_word_start(bytes: &[u8], index: usize) -> bool {
    index > 0 && bytes[index - 1].is_ascii_whitespace()
}

fn skip_quoted(bytes: &[u8], open: usize) -> usize {
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

/// Splits a trailing ` as <type>` off a declaration value.
fn split_declared_as(value: &str) -> (String, Option<String>) {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut candidate = None;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            index = skip_quoted(bytes, index);
            continue;
        }
        if at_word_start(bytes, index) && word_is(value, index, "as") {
            candidate = Some(index);
        }
        index += 1;
    }
    let Some(start) = candidate else {
        return (value.trim().to_string(), None);
    };
    let written = value[start + 2..].trim();
    let identifier = !written.is_empty()
        && written
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if identifier {
        (value[..start].trim().to_string(), Some(written.to_string()))
    } else {
        (value.trim().to_string(), None)
    }
}

/// Reads a Yarn literal without evaluating it.
fn infer_literal(text: &str) -> (VariableType, Value) {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return (VariableType::Bool, Value::Bool(true));
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return (VariableType::Bool, Value::Bool(false));
    }
    if let Some(inner) = quoted_string(trimmed) {
        return (VariableType::String, Value::String(unescape(inner)));
    }
    let unknown = (VariableType::Unknown, Value::String(trimmed.to_string()));
    match numeric_form(trimmed) {
        NumericForm::Integer => match trimmed.parse::<i64>() {
            Ok(value) => (VariableType::Int, Value::from(value)),
            Err(_) => unknown,
        },
        NumericForm::Decimal => {
            let parsed = trimmed
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .and_then(serde_json::Number::from_f64);
            match parsed {
                Some(value) => (VariableType::Float, Value::from(value)),
                None => unknown,
            }
        }
        NumericForm::NotNumeric => unknown,
    }
}

enum NumericForm {
    Integer,
    Decimal,
    NotNumeric,
}

fn numeric_form(text: &str) -> NumericForm {
    let bytes = text.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'-') | Some(b'+')));
    let mut digits = 0;
    let mut decimal = false;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        digits += 1;
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        decimal = true;
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            digits += 1;
            index += 1;
        }
    }
    if digits == 0 {
        return NumericForm::NotNumeric;
    }
    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        decimal = true;
        index += 1;
        if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
            index += 1;
        }
        let mut exponent = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            exponent += 1;
            index += 1;
        }
        if exponent == 0 {
            return NumericForm::NotNumeric;
        }
    }
    if index != bytes.len() {
        return NumericForm::NotNumeric;
    }
    if decimal {
        NumericForm::Decimal
    } else {
        NumericForm::Integer
    }
}

fn quoted_string(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == '"' {
            return None;
        }
    }
    Some(inner)
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}
