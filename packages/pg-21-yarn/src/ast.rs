// SPDX-License-Identifier: MIT OR Apache-2.0

//! Private syntax records retained by the Yarn parser.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedSourceSet {
    /// Input names in command-line order; `nodes[].source` indexes this list.
    pub sources: Vec<String>,
    /// One entry per Yarn `title:` block, in source order.
    pub titles: Vec<Title>,
    /// One entry per statement, in source order.
    pub nodes: Vec<Node>,
    pub variables: Vec<Variable>,
    /// Errors and warnings together; `severity` separates them.
    pub errors: Vec<Finding>,
}

impl ParsedSourceSet {
    pub(crate) fn new(sources: Vec<String>) -> Self {
        Self {
            sources,
            titles: Vec::new(),
            nodes: Vec::new(),
            variables: Vec::new(),
            errors: Vec::new(),
        }
    }
}

/// A Yarn node: the `title:` block and its headers.
#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    /// Unique key used as the prefix of every statement id in this block.
    pub id: String,
    /// The `title:` header value, or `untitled` when the header was missing.
    pub title: String,
    /// Index into [`ParsedSourceSet::sources`].
    pub source: usize,

    pub tags: Vec<String>,
    /// Every header other than `title` and `tags`, values kept as written.
    pub meta: Map<String, Value>,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]

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

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// `"<title id>:<ordinal>"`, unique across the whole document.
    pub id: String,
    /// Index into [`ParsedSourceSet::sources`].
    pub source: usize,
    /// Owning Yarn node, matching a [`Title::id`].
    pub title: String,
    pub kind: NodeKind,
    /// `Character:` prefix, when the line had one.
    pub speaker: Option<String>,
    /// Dialogue or option text, verbatim: `{$expr}` and `[markup]` are kept.
    pub text: Option<String>,
    /// Value of the `#line:` hashtag, without the `line:` prefix.
    pub line_id: Option<String>,
    /// Every other hashtag, in source order, without the leading `#`.
    pub tags: Vec<String>,
    /// True when `text` contains at least one `[markup]` tag.
    pub markup: bool,
    /// Trailing `<<if expr>>` on an option, which gates its availability.
    pub condition: Option<String>,
    /// 1-based choice-group index within the owning Yarn node.
    pub option_group: Option<usize>,
    /// Nearest enclosing `options` or `conditional` node; `null` at top level.
    pub parent: Option<String>,
    /// Clause label when `parent` is a `conditional` node.
    pub clause: Option<String>,

    pub command: Option<Command>,
    /// 1-based source position of the statement.
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// Command word exactly as written; matching is case-insensitive.
    pub name: String,
    /// Everything after the command word, verbatim and trimmed.
    pub args: Option<String>,
    /// `$`-stripped variable name for `set` and `declare`.
    pub variable: Option<String>,
    /// `=`, `+=`, `-=`, `*=`, `/=` or `to` for `set`; `=` for `declare`.
    pub operator: Option<String>,
    /// Right-hand side or condition text, verbatim; never evaluated.
    pub expression: Option<String>,
    /// Jump destination title, verbatim.
    pub target: Option<String>,

    pub clauses: Vec<Clause>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    pub keyword: ClauseKeyword,
    /// `if`, `else`, or `elseif 1` for the first `<<elseif>>`.
    pub label: String,
    /// Condition text, verbatim; absent for `<<else>>`.
    pub expression: Option<String>,
    /// Node ids reachable when the clause is taken, in source order.
    pub body: Vec<String>,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]

pub enum ClauseKeyword {
    If,
    ElseIf,
    Else,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]

pub enum VariableType {
    Bool,
    Int,
    Float,
    String,
    /// The initial value is not a literal this crate recognizes.
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    /// Name without the leading `$`.
    pub name: String,

    pub declared_type: VariableType,
    /// Type written in an `as <type>` suffix, when present.
    pub declared_as: Option<String>,
    /// Initial value as JSON, or the verbatim text when the type is unknown.
    pub value: Value,
    pub raw_value: String,
    /// `///` doc-comment lines immediately above the declaration.
    pub description: Option<String>,
    /// Index into [`ParsedSourceSet::sources`].
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
            Self::DuplicateNodeTitle
            | Self::DuplicateHeaderKey
            | Self::InvalidVariableType
            | Self::TrailingContentAfterCommand
            | Self::UnknownCommand => Severity::Warning,
            Self::BadCommandSyntax
            | Self::ContentOutsideNode
            | Self::DuplicateLineId
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
