// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DialogueGraph {
    pub version: u8,
    pub text_mode: TextMode,
    pub metadata: Map<String, Value>,
    pub definitions: Vec<Value>,
    pub packages: Vec<Package>,
    /// All models, including entities, folders, assets and unknown object kinds.
    pub nodes: Vec<Node>,
    pub pins: Vec<Pin>,
    /// Connection records and jump transfers in source order, not an execution plan.
    pub edges: Vec<Edge>,
    /// Branch candidates. Availability/player-vs-system selection is not evaluated.
    pub choices: Vec<Choice>,
    pub variable_namespaces: Vec<VariableNamespace>,
    pub variables: Vec<Variable>,
    pub hierarchy: Vec<HierarchyEntry>,
    pub warnings: Vec<Warning>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMode {
    Literal,
    LocalizationKeys,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Package {
    pub index: usize,
    pub name: String,
    pub metadata: Map<String, Value>,
    pub node_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    FlowFragment,
    Dialogue,
    DialogueFragment,
    Hub,
    Jump,
    Condition,
    Instruction,
    Entity,
    UserFolder,
    Comment,
    Asset,
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Node {
    pub id: String,
    pub package: usize,
    pub source_type: String,
    pub kind: NodeKind,
    pub parent: Option<String>,
    pub technical_name: Option<String>,
    pub display_name: Option<String>,
    pub text: Option<String>,
    pub menu_text: Option<String>,
    pub speaker: Option<String>,
    pub script: Option<Script>,
    pub input_pins: Vec<String>,
    pub output_pins: Vec<String>,
    /// All source properties except embedded pins, which are stored separately.
    pub properties: Map<String, Value>,
    pub template: Option<Value>,
    /// Remaining model-level fields such as AssetRef, without reading the asset.
    pub metadata: Map<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PinDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRole {
    Condition,
    Instruction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Script {
    pub language: String,
    pub role: ScriptRole,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Pin {
    pub id: String,
    pub owner: String,
    pub direction: PinDirection,
    pub index: usize,
    pub script: Option<Script>,
    /// All source pin fields except connection records, which become edges.
    pub properties: Map<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Connection,
    Jump,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Edge {
    /// Generated stable reading-order ID, never substituted for a source object ID.
    pub id: String,
    pub kind: EdgeKind,
    pub source: String,
    pub source_pin: Option<String>,
    pub target: String,
    pub target_pin: Option<String>,
    pub index: usize,
    pub label: Option<String>,
    pub properties: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Choice {
    pub edge: String,
    pub source: String,
    pub source_pin: String,
    pub target: String,
    /// Text is referenced, not copied for every candidate pointing to a large label.
    pub text_source: ChoiceTextSource,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceTextSource {
    EdgeLabel,
    TargetMenuText,
    TargetText,
    None,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VariableNamespace {
    pub name: String,
    pub metadata: Map<String, Value>,
    pub variables: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    Boolean,
    Integer,
    Float,
    String,
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Variable {
    pub namespace: String,
    pub name: String,
    pub source_type: String,
    pub kind: VariableKind,
    pub value: Value,
    pub raw_value: Value,
    pub description: Option<String>,
    pub metadata: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HierarchyEntry {
    pub id: String,
    pub parent: Option<String>,
    pub index: usize,
    pub depth: usize,
    pub properties: Map<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    MissingNode,
    MissingPin,
    MissingParent,
    MissingSpeaker,
    MissingHierarchyObject,
    HierarchyParentMismatch,
    UnknownType,
    UnknownVariableType,
    MissingScript,
    LocalizationKeys,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Warning {
    pub kind: WarningKind,
    pub source: Option<String>,
    pub reference: Option<String>,
}
