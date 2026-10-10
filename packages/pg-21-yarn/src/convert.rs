// SPDX-License-Identifier: MIT OR Apache-2.0

//! Converts the private Yarn syntax tree to the local PG-03 v1 contract.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{ast, model::*};

pub use ast::Finding as ReportFinding;

/// Source diagnostics are serialized separately from the neutral graph.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ParseReport {
    pub sources: Vec<String>,
    pub findings: Vec<ReportFinding>,
}

impl ParseReport {
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == ast::Severity::Error)
            .count()
    }
    pub fn finding_count(&self) -> usize {
        self.findings.len()
    }
}

#[derive(Clone)]
enum FlowNode {
    Single(usize),
    Group(usize, Vec<(usize, Vec<FlowNode>)>),
    Branch(usize, Vec<(String, Vec<FlowNode>)>),
}

impl FlowNode {
    fn entry(&self) -> usize {
        match self {
            Self::Single(n) | Self::Group(n, _) | Self::Branch(n, _) => *n,
        }
    }
}

struct Converter {
    graph: DialogueGraph,
    issued: HashSet<String>,
    siblings: HashMap<Option<String>, usize>,
    depths: HashMap<String, usize>,
    titles: HashMap<String, usize>,
    outgoing: HashMap<usize, usize>,
    jumps: HashMap<usize, String>,
    children: HashMap<(String, Option<String>, Option<String>), Vec<usize>>,
}

pub(crate) fn convert(old: ast::ParsedSourceSet) -> (DialogueGraph, ParseReport) {
    let mut report = ParseReport {
        sources: old.sources.clone(),
        findings: old.errors.clone(),
    };
    let packages = old
        .sources
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let path = safe_path(name);
            Package {
                index,
                name: Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("stdin")
                    .to_string(),
                metadata: Map::from_iter([("path".into(), json!(path))]),
                node_ids: Vec::new(),
            }
        })
        .collect();
    let mut children: HashMap<_, Vec<usize>> = HashMap::new();
    for (index, node) in old.nodes.iter().enumerate() {
        children
            .entry((node.title.clone(), node.parent.clone(), node.clause.clone()))
            .or_default()
            .push(index);
    }
    let mut c = Converter {
        graph: DialogueGraph {
            version: 1,
            text_mode: TextMode::Literal,
            metadata: Map::from_iter([
                ("source_format".into(), json!("yarn")),
                (
                    "generator".into(),
                    json!(concat!(
                        env!("CARGO_PKG_NAME"),
                        " ",
                        env!("CARGO_PKG_VERSION")
                    )),
                ),
                (
                    "files".into(),
                    json!(old.sources.iter().map(|s| safe_path(s)).collect::<Vec<_>>()),
                ),
            ]),
            definitions: Vec::new(),
            packages,
            nodes: Vec::new(),
            pins: Vec::new(),
            edges: Vec::new(),
            choices: Vec::new(),
            variable_namespaces: Vec::new(),
            variables: Vec::new(),
            hierarchy: Vec::new(),
            warnings: Vec::new(),
        },
        issued: HashSet::new(),
        siblings: HashMap::new(),
        depths: HashMap::new(),
        titles: HashMap::new(),
        outgoing: HashMap::new(),
        jumps: HashMap::new(),
        children,
    };
    let mut bodies = Vec::new();
    for title in &old.titles {
        let id = c.issue(&title.title);
        if id != title.title
            && !report.findings.iter().any(|f| {
                f.file == old.sources[title.source]
                    && f.line == title.line
                    && f.code == ast::FindingCode::DuplicateNodeTitle
            })
        {
            report.findings.push(ReportFinding {
                file: old.sources[title.source].clone(),
                line: title.line,
                col: title.col,
                code: ast::FindingCode::DuplicateNodeTitle,
                severity: ast::Severity::Warning,
                message: format!(
                    "node title `{}` collides with an issued ID; this block becomes `{id}`",
                    title.title
                ),
            });
        }
        let index = c.add_node(id, title.source, "Node", NodeKind::FlowFragment, None);
        c.titles.entry(title.title.clone()).or_insert(index);
        let node = &mut c.graph.nodes[index];
        node.technical_name = Some(title.title.clone());
        node.display_name = Some(title.title.clone());
        node.properties.insert("tags".into(), json!(title.tags));
        node.properties.insert("headers".into(), json!(title.meta));
        node.metadata = position(title.line, title.col);
        let body = c.block(&old, &title.id, None, index, None);
        bodies.push((index, body));
    }
    for variable in &old.variables {
        c.graph.variables.push(Variable {
            namespace: "yarn".into(),
            name: variable.name.clone(),
            source_type: "declare".into(),
            kind: match variable.declared_type {
                ast::VariableType::Bool => VariableKind::Boolean,
                ast::VariableType::Int => VariableKind::Integer,
                ast::VariableType::Float => VariableKind::Float,
                ast::VariableType::String => VariableKind::String,
                ast::VariableType::Unknown => VariableKind::Other,
            },
            value: variable.value.clone(),
            raw_value: json!(variable.raw_value),
            description: variable.description.clone(),
            metadata: Map::new(),
        });
    }
    c.graph.variable_namespaces.push(VariableNamespace {
        name: "yarn".into(),
        metadata: Map::new(),
        variables: c.graph.variables.iter().map(|v| v.name.clone()).collect(),
    });
    for (container, body) in bodies {
        if let Some(first) = body.first() {
            c.edge(container, first.entry(), EdgeKind::Connection, None);
        }
        c.flow(&body, None);
    }
    for finding in &report.findings {
        let kind = match finding.code {
            ast::FindingCode::MissingJumpTarget => Some(WarningKind::MissingNode),
            ast::FindingCode::UnknownCommand | ast::FindingCode::DuplicateNodeTitle => {
                Some(WarningKind::UnknownType)
            }
            ast::FindingCode::InvalidVariableType => Some(WarningKind::UnknownVariableType),
            _ => None,
        };
        if let Some(kind) = kind {
            // Missing references are added with their raw target during flow emission.
            if kind != WarningKind::MissingNode {
                c.graph.warnings.push(Warning {
                    kind,
                    source: Some(safe_path(&finding.file)),
                    reference: None,
                });
            }
        }
    }
    report.findings.sort_by_key(|f| {
        (
            old.sources.iter().position(|s| s == &f.file).unwrap_or(0),
            f.line,
            f.col,
            f.code,
        )
    });
    (c.graph, report)
}

fn safe_path(name: &str) -> String {
    let path = Path::new(name);
    if path.is_absolute() {
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("stdin")
            .to_string()
    } else {
        name.to_string()
    }
}

fn position(line: u32, col: u32) -> Map<String, Value> {
    Map::from_iter([("line".into(), json!(line)), ("col".into(), json!(col))])
}

impl Converter {
    fn issue(&mut self, base: &str) -> String {
        let mut id = base.to_string();
        let mut suffix = 2;
        while self.issued.contains(&id)
            || self.issued.contains(&format!("{id}#in"))
            || self.issued.contains(&format!("{id}#out"))
        {
            id = format!("{base}~{suffix}");
            suffix += 1;
        }
        self.issued.insert(id.clone());
        self.issued.insert(format!("{id}#in"));
        self.issued.insert(format!("{id}#out"));
        id
    }

    fn child(&mut self, parent: usize, package: usize, source_type: &str, kind: NodeKind) -> usize {
        let parent_id = self.graph.nodes[parent].id.clone();
        let ordinal = self
            .siblings
            .get(&Some(parent_id.clone()))
            .copied()
            .unwrap_or(0)
            + 1;
        let id = self.issue(&format!("{parent_id}:{ordinal}"));
        self.add_node(id, package, source_type, kind, Some(parent_id))
    }

    fn add_node(
        &mut self,
        id: String,
        package: usize,
        source_type: &str,
        kind: NodeKind,
        parent: Option<String>,
    ) -> usize {
        let index = self.graph.nodes.len();
        let sibling = self.siblings.entry(parent.clone()).or_default();
        let sibling_index = *sibling;
        *sibling += 1;
        let depth = parent.as_ref().map_or(0, |p| self.depths[p] + 1);
        self.depths.insert(id.clone(), depth);
        self.graph.hierarchy.push(HierarchyEntry {
            id: id.clone(),
            parent: parent.clone(),
            index: sibling_index,
            depth,
            properties: Map::new(),
        });
        self.graph.packages[package].node_ids.push(id.clone());
        let input = format!("{id}#in");
        let output = format!("{id}#out");
        for (pin, direction) in [
            (&input, PinDirection::Input),
            (&output, PinDirection::Output),
        ] {
            self.graph.pins.push(Pin {
                id: pin.clone(),
                owner: id.clone(),
                direction,
                index: 0,
                script: None,
                properties: Map::new(),
            });
        }
        self.graph.nodes.push(Node {
            id,
            package,
            source_type: source_type.into(),
            kind,
            parent,
            technical_name: None,
            display_name: None,
            text: None,
            menu_text: None,
            speaker: None,
            script: None,
            input_pins: vec![input],
            output_pins: vec![output],
            properties: Map::new(),
            template: None,
            metadata: Map::new(),
        });
        index
    }

    fn block(
        &mut self,
        old: &ast::ParsedSourceSet,
        title: &str,
        old_parent: Option<&str>,
        parent: usize,
        clause: Option<&str>,
    ) -> Vec<FlowNode> {
        let key = (
            title.to_string(),
            old_parent.map(str::to_string),
            clause.map(str::to_string),
        );
        let statements: Vec<_> = self
            .children
            .get(&key)
            .into_iter()
            .flatten()
            .map(|index| &old.nodes[*index])
            .collect();
        let mut body = Vec::new();
        let mut cursor = 0;
        while let Some(statement) = statements.get(cursor) {
            if statement
                .command
                .as_ref()
                .is_some_and(|c| c.name.eq_ignore_ascii_case("declare"))
            {
                cursor += 1;
                continue;
            }
            if statement.kind == ast::NodeKind::Options {
                let hub = self.child(parent, statement.source, "OptionGroup", NodeKind::Hub);
                let node = &mut self.graph.nodes[hub];
                node.properties
                    .insert("index".into(), json!(statement.option_group));
                node.metadata = position(statement.line, statement.col);
                if let Some(clause) = clause {
                    node.properties.insert("clause".into(), json!(clause));
                }
                let group = statement.option_group;
                let mut options = Vec::new();
                while let Some(option) = statements
                    .get(cursor)
                    .filter(|n| n.kind == ast::NodeKind::Options && n.option_group == group)
                {
                    let index = self.statement(option, hub, clause);
                    let children = self.block(old, title, Some(&option.id), index, None);
                    options.push((index, children));
                    cursor += 1;
                }
                body.push(FlowNode::Group(hub, options));
            } else {
                let index = self.statement(statement, parent, clause);
                if statement.kind == ast::NodeKind::Conditional {
                    let mut branches = Vec::new();
                    if let Some(command) = &statement.command {
                        for branch in &command.clauses {
                            let children = self.block(
                                old,
                                title,
                                Some(&statement.id),
                                index,
                                Some(&branch.label),
                            );
                            branches.push((branch.label.clone(), children));
                        }
                    }
                    body.push(FlowNode::Branch(index, branches));
                } else {
                    body.push(FlowNode::Single(index));
                }
                cursor += 1;
            }
        }
        body
    }

    fn statement(&mut self, old: &ast::Node, parent: usize, clause: Option<&str>) -> usize {
        let command = old.command.as_ref();
        let name = command.map_or("", |c| c.name.as_str());
        let (kind, source_type) = match old.kind {
            ast::NodeKind::Line => (NodeKind::DialogueFragment, "Line"),
            ast::NodeKind::Options => (NodeKind::DialogueFragment, "Option"),
            ast::NodeKind::Conditional => (NodeKind::Condition, "If"),
            ast::NodeKind::Jump => (NodeKind::Jump, "Jump"),
            ast::NodeKind::Command if name.eq_ignore_ascii_case("stop") => (NodeKind::Jump, "Stop"),
            ast::NodeKind::Command if name.eq_ignore_ascii_case("set") => {
                (NodeKind::Instruction, "Set")
            }
            ast::NodeKind::Command => (NodeKind::Instruction, "Command"),
        };
        let index = self.child(parent, old.source, source_type, kind);
        let node = &mut self.graph.nodes[index];
        node.metadata = position(old.line, old.col);
        node.speaker = old.speaker.clone();
        if source_type == "Option" {
            node.menu_text = old.text.clone();
        } else {
            node.text = old.text.clone();
        }
        if matches!(source_type, "Line" | "Option") {
            node.properties.insert("line_id".into(), json!(old.line_id));
            node.properties.insert("tags".into(), json!(old.tags));
            node.properties.insert("markup".into(), json!(old.markup));
            if let Some(condition) = &old.condition {
                node.properties.insert("condition".into(), json!(condition));
            }
        }
        if let Some(clause) = clause {
            node.properties.insert("clause".into(), json!(clause));
        }
        if let Some(command) = command {
            if kind == NodeKind::Jump {
                let target = if source_type == "Stop" {
                    "stop".into()
                } else {
                    command.target.clone().unwrap_or_default()
                };
                node.properties.insert("target".into(), json!(target));
                if source_type != "Stop" && !target.is_empty() {
                    self.jumps.insert(index, target);
                }
            } else {
                node.script = Some(Script {
                    language: "yarn".into(),
                    role: if kind == NodeKind::Condition {
                        ScriptRole::Condition
                    } else {
                        ScriptRole::Instruction
                    },
                    text: if kind == NodeKind::Condition {
                        command.expression.clone().unwrap_or_default()
                    } else {
                        format!(
                            "{}{}",
                            command.name,
                            command
                                .args
                                .as_ref()
                                .map_or(String::new(), |args| format!(" {args}"))
                        )
                    },
                });
                if kind == NodeKind::Instruction {
                    node.properties
                        .insert("command".into(), json!(command.name));
                    node.properties.insert("args".into(), json!(command.args));
                    if let Some(variable) = &command.variable {
                        node.properties.insert("variable".into(), json!(variable));
                    }
                    if let Some(operator) = &command.operator {
                        node.properties.insert("operator".into(), json!(operator));
                    }
                    if let Some(expression) = &command.expression {
                        node.properties
                            .insert("expression".into(), json!(expression));
                    }
                }
            }
        }
        index
    }

    fn edge(
        &mut self,
        source: usize,
        target: usize,
        kind: EdgeKind,
        label: Option<String>,
    ) -> String {
        let id = format!("e{}", self.graph.edges.len());
        let index = self.outgoing.entry(source).or_default();
        let source_node = &self.graph.nodes[source];
        let target_node = &self.graph.nodes[target];
        self.graph.edges.push(Edge {
            id: id.clone(),
            kind,
            source: source_node.id.clone(),
            source_pin: Some(source_node.output_pins[0].clone()),
            target: target_node.id.clone(),
            target_pin: Some(target_node.input_pins[0].clone()),
            index: *index,
            label,
            properties: Map::new(),
        });
        *index += 1;
        id
    }

    fn flow(&mut self, body: &[FlowNode], following: Option<usize>) {
        for (position, item) in body.iter().enumerate() {
            let next = body.get(position + 1).map(FlowNode::entry).or(following);
            match item {
                FlowNode::Single(index) => {
                    if self.graph.nodes[*index].kind == NodeKind::Jump {
                        if let Some(target) = self.jumps.get(index).cloned() {
                            if target.starts_with('{') && target.ends_with('}') {
                                continue;
                            }
                            if let Some(destination) = self.titles.get(&target).copied() {
                                self.edge(*index, destination, EdgeKind::Jump, None);
                            } else {
                                self.graph.warnings.push(Warning {
                                    kind: WarningKind::MissingNode,
                                    source: Some(self.graph.nodes[*index].id.clone()),
                                    reference: Some(target),
                                });
                            }
                        }
                    } else if let Some(next) = next {
                        self.edge(*index, next, EdgeKind::Connection, None);
                    }
                }
                FlowNode::Group(hub, options) => {
                    for (option, children) in options {
                        let edge = self.edge(*hub, *option, EdgeKind::Connection, None);
                        self.graph.choices.push(Choice {
                            edge,
                            source: self.graph.nodes[*hub].id.clone(),
                            source_pin: self.graph.nodes[*hub].output_pins[0].clone(),
                            target: self.graph.nodes[*option].id.clone(),
                            text_source: ChoiceTextSource::TargetMenuText,
                        });
                        if let Some(child) = children.first() {
                            self.edge(*option, child.entry(), EdgeKind::Connection, None);
                        } else if let Some(next) = next {
                            self.edge(*option, next, EdgeKind::Connection, None);
                        }
                        self.flow(children, next);
                    }
                }
                FlowNode::Branch(condition, branches) => {
                    for (label, children) in branches {
                        if let Some(target) = children.first().map(FlowNode::entry).or(next) {
                            self.edge(
                                *condition,
                                target,
                                EdgeKind::Connection,
                                Some(label.clone()),
                            );
                        }
                        self.flow(children, next);
                    }
                    if !branches.iter().any(|(label, _)| label == "else") {
                        if let Some(next) = next {
                            self.edge(*condition, next, EdgeKind::Connection, Some("else".into()));
                        }
                    }
                }
            }
        }
    }
}
