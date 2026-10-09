// SPDX-License-Identifier: MIT OR Apache-2.0

//! The neutral dialogue-graph document, version 1.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Current document version. Every graph this crate emits carries it.
pub const GRAPH_VERSION: u8 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DialogueGraph {
    pub version: u8,
    /// Input names in command-line order; `nodes[].source` indexes this list.
    pub sources: Vec<String>,
    /// Node id the runtime should enter, or `null` when nothing is parseable.
    pub start_node: Option<String>,
    /// One entry per Yarn `title:` block, in source order.
    pub titles: Vec<Title>,
    /// One entry per statement, in source order.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub variables: Vec<Variable>,
    /// Errors and warnings together; `severity` separates them.
    pub errors: Vec<Finding>,
}

impl DialogueGraph {
    pub(crate) fn new(sources: Vec<String>) -> Self {
        Self {
            version: GRAPH_VERSION,
            sources,
            start_node: None,
            titles: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            variables: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Number of findings carrying [`Severity::Error`].
    pub fn error_count(&self) -> usize {
        self.errors
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    /// Total number of findings, errors and warnings alike.
    pub fn finding_count(&self) -> usize {
        self.errors.len()
    }
}

/// A Yarn node: the `title:` block and its headers.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Title {
    /// Unique key used as the prefix of every statement id in this block.
    pub id: String,
    /// The `title:` header value, or `untitled` when the header was missing.
    pub title: String,
    /// Index into [`DialogueGraph::sources`].
    pub source: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Every header other than `title` and `tags`, values kept as written.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub meta: Map<String, Value>,
    /// Node id of the first statement, or `null` for an empty body.
    pub entry: Option<String>,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A line of dialogue or narration.
    Line,
    /// One `->` shortcut option.
    Options,
    /// `<<set>>`, `<<declare>>`, `<<stop>>`, `<<wait>>` or any custom command.
    Command,
    /// `<<jump Target>>`.
    Jump,
    /// A whole `<<if>> ... <<endif>>` block.
    Conditional,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Node {
    /// `"<title id>:<ordinal>"`, unique across the whole document.
    pub id: String,
    /// Index into [`DialogueGraph::sources`].
    pub source: usize,
    /// Owning Yarn node, matching a [`Title::id`].
    pub title: String,
    pub kind: NodeKind,
    /// `Character:` prefix, when the line had one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// Dialogue or option text, verbatim: `{$expr}` and `[markup]` are kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Value of the `#line:` hashtag, without the `line:` prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_id: Option<String>,
    /// Every other hashtag, in source order, without the leading `#`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// True when `text` contains at least one `[markup]` tag.
    #[serde(default, skip_serializing_if = "is_false")]
    pub markup: bool,
    /// Trailing `<<if expr>>` on an option, which gates its availability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// 1-based choice-group index within the owning Yarn node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_group: Option<usize>,
    /// Nearest enclosing `options` or `conditional` node; `null` at top level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Clause label when `parent` is a `conditional` node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
    /// 1-based source position of the statement.
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Command {
    /// Command word exactly as written; matching is case-insensitive.
    pub name: String,
    /// Everything after the command word, verbatim and trimmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    /// `$`-stripped variable name for `set` and `declare`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    /// `=`, `+=`, `-=`, `*=`, `/=` or `to` for `set`; `=` for `declare`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    /// Right-hand side or condition text, verbatim; never evaluated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// Jump destination title, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clauses: Vec<Clause>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Clause {
    pub keyword: ClauseKeyword,
    /// `if`, `else`, or `elseif 1` for the first `<<elseif>>`.
    pub label: String,
    /// Condition text, verbatim; absent for `<<else>>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// Node ids reachable when the clause is taken, in source order.
    pub body: Vec<String>,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClauseKeyword {
    If,
    ElseIf,
    Else,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Sequential flow inside one block.
    Flow,
    /// From the node presenting a choice to each of its options.
    Option,
    /// From a `conditional` node into one of its clause bodies.
    Branch,
    /// From a `jump` node to the entry of the target Yarn node.
    Jump,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Edge {
    /// Generated `edge-1`, `edge-2`, ... in traversal order.
    pub id: String,
    pub kind: EdgeKind,
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// 0-based position in [`DialogueGraph::edges`].
    pub index: usize,
    /// Clause label on `branch` edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableType {
    Bool,
    Int,
    Float,
    String,
    /// The initial value is not a literal this crate recognizes.
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Variable {
    /// Name without the leading `$`.
    pub name: String,
    #[serde(rename = "type")]
    pub declared_type: VariableType,
    /// Type written in an `as <type>` suffix, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_as: Option<String>,
    /// Initial value as JSON, or the verbatim text when the type is unknown.
    pub value: Value,
    /// `///` doc-comment lines immediately above the declaration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Index into [`DialogueGraph::sources`].
    pub source: usize,
    /// Owning Yarn node, matching a [`Title::id`].
    pub title: String,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

/// Stable finding codes. Adding a variant is a compatible change; renaming or
/// removing one is not.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    BadCommandSyntax,
    ContentOutsideNode,
    DuplicateHeaderKey,
    DuplicateLineId,
    DuplicateNodeTitle,
    DuplicateVariable,
    EmptyLineId,
    InvalidVariableType,
    MalformedHashtag,
    MissingHeaderDelimiter,
    MissingJumpTarget,
    MissingNodeTitle,
    NestingTooDeep,
    OptionAfterNodeEnd,
    TrailingContentAfterCommand,
    UnbalancedConditional,
    UnexpectedHeaderDelimiter,
    UnexpectedNodeEnd,
    UnknownCommand,
    UnterminatedCommand,
}

impl FindingCode {
    /// Whether the code blocks a non-strict run.
    pub fn severity(self) -> Severity {
        match self {
            Self::DuplicateHeaderKey
            | Self::InvalidVariableType
            | Self::TrailingContentAfterCommand
            | Self::UnknownCommand => Severity::Warning,
            Self::BadCommandSyntax
            | Self::ContentOutsideNode
            | Self::DuplicateLineId
            | Self::DuplicateNodeTitle
            | Self::DuplicateVariable
            | Self::EmptyLineId
            | Self::MalformedHashtag
            | Self::MissingHeaderDelimiter
            | Self::MissingJumpTarget
            | Self::MissingNodeTitle
            | Self::NestingTooDeep
            | Self::OptionAfterNodeEnd
            | Self::UnbalancedConditional
            | Self::UnexpectedHeaderDelimiter
            | Self::UnexpectedNodeEnd
            | Self::UnterminatedCommand => Severity::Error,
        }
    }

    /// The code as it appears in JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadCommandSyntax => "bad_command_syntax",
            Self::ContentOutsideNode => "content_outside_node",
            Self::DuplicateHeaderKey => "duplicate_header_key",
            Self::DuplicateLineId => "duplicate_line_id",
            Self::DuplicateNodeTitle => "duplicate_node_title",
            Self::DuplicateVariable => "duplicate_variable",
            Self::EmptyLineId => "empty_line_id",
            Self::InvalidVariableType => "invalid_variable_type",
            Self::MalformedHashtag => "malformed_hashtag",
            Self::MissingHeaderDelimiter => "missing_header_delimiter",
            Self::MissingJumpTarget => "missing_jump_target",
            Self::MissingNodeTitle => "missing_node_title",
            Self::NestingTooDeep => "nesting_too_deep",
            Self::OptionAfterNodeEnd => "option_after_node_end",
            Self::TrailingContentAfterCommand => "trailing_content_after_command",
            Self::UnbalancedConditional => "unbalanced_conditional",
            Self::UnexpectedHeaderDelimiter => "unexpected_header_delimiter",
            Self::UnexpectedNodeEnd => "unexpected_node_end",
            Self::UnknownCommand => "unknown_command",
            Self::UnterminatedCommand => "unterminated_command",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Finding {
    /// Source name exactly as it was passed in.
    pub file: String,
    pub line: u32,
    /// 1-based character offset within the line.
    pub col: u32,
    pub code: FindingCode,
    pub severity: Severity,
    /// Human-readable text. Quotes only identifiers — node titles, command
    /// names, line ids, type words — never dialogue prose from the source.
    pub message: String,
}

fn is_false(value: &bool) -> bool {
    !*value
}
