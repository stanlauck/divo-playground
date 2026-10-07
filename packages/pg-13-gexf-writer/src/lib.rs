// SPDX-License-Identifier: MIT OR Apache-2.0

//! A small, validated writer for typed GEXF 1.3 graphs.
//!
//! The crate models node and edge attributes, dynamic attribute values, and
//! dynamic topology spells. It writes XML directly and makes no network calls.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{self, Write};

const GEXF_NAMESPACE: &str = "http://gexf.net/1.3";

/// A complete graph to serialize as GEXF 1.3.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Graph {
    pub mode: GraphMode,
    #[serde(default)]
    pub time_format: Option<TimeFormat>,
    pub default_edge_type: DefaultEdgeType,
    #[serde(default)]
    pub attributes: Vec<AttributeDefinition>,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphMode {
    Static,
    Dynamic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
    Integer,
    Double,
    Date,
    DateTime,
}

impl TimeFormat {
    fn as_gexf(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Double => "double",
            Self::Date => "date",
            Self::DateTime => "dateTime",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeClass {
    Node,
    Edge,
}

impl AttributeClass {
    fn as_gexf(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Edge => "edge",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeMode {
    Static,
    Dynamic,
}

impl AttributeMode {
    fn as_gexf(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Dynamic => "dynamic",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultEdgeType {
    Directed,
    Undirected,
    Mutual,
}

impl DefaultEdgeType {
    fn as_gexf(self) -> &'static str {
        match self {
            Self::Directed => "directed",
            Self::Undirected => "undirected",
            Self::Mutual => "mutual",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Integer,
    Long,
    Double,
    Float,
    Boolean,
    String,
    ListString,
}

impl ValueType {
    fn as_gexf(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Long => "long",
            Self::Double => "double",
            Self::Float => "float",
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::ListString => "liststring",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AttributeDefinition {
    pub id: String,
    pub title: String,
    pub class: AttributeClass,
    pub mode: AttributeMode,
    pub value_type: ValueType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<AttributeValue>,
}

/// A value tagged with its explicit JSON type.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AttributeValue {
    Integer(i32),
    Long(i64),
    Double(f64),
    Float(f32),
    Boolean(bool),
    String(String),
    ListString(Vec<String>),
}

impl AttributeValue {
    fn value_type(&self) -> ValueType {
        match self {
            Self::Integer(_) => ValueType::Integer,
            Self::Long(_) => ValueType::Long,
            Self::Double(_) => ValueType::Double,
            Self::Float(_) => ValueType::Float,
            Self::Boolean(_) => ValueType::Boolean,
            Self::String(_) => ValueType::String,
            Self::ListString(_) => ValueType::ListString,
        }
    }

    fn lexical_value(&self) -> Result<String, ValidationError> {
        match self {
            Self::Integer(value) => Ok(value.to_string()),
            Self::Long(value) => Ok(value.to_string()),
            Self::Double(value) if value.is_finite() => Ok(value.to_string()),
            Self::Float(value) if value.is_finite() => Ok(value.to_string()),
            Self::Double(_) | Self::Float(_) => Err(ValidationError(
                "attribute values must be finite numbers".into(),
            )),
            Self::Boolean(value) => Ok(value.to_string()),
            Self::String(value) => Ok(value.clone()),
            Self::ListString(values) => {
                if values.iter().any(|value| {
                    value
                        .chars()
                        .any(|character| matches!(character, '|' | ',' | ';'))
                }) {
                    return Err(ValidationError(
                        "liststring items cannot contain '|', ',' or ';'".into(),
                    ));
                }
                Ok(values.join("|"))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Node {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub attributes: Vec<TimedAttributeValue>,
    #[serde(default)]
    pub spells: Vec<TimeInterval>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub attributes: Vec<TimedAttributeValue>,
    #[serde(default)]
    pub spells: Vec<TimeInterval>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TimedAttributeValue {
    pub attribute_id: String,
    pub value: AttributeValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<TimeInterval>,
}

/// An inclusive or open-ended GEXF time interval.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TimeInterval {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub start_open: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub end_open: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError(pub String);

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug)]
pub enum GexfError {
    Validation(ValidationError),
    Io(io::Error),
}

impl fmt::Display for GexfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => write!(formatter, "invalid graph: {error}"),
            Self::Io(error) => write!(formatter, "could not write GEXF: {error}"),
        }
    }
}

impl std::error::Error for GexfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<ValidationError> for GexfError {
    fn from(error: ValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<io::Error> for GexfError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Graph {
    /// Validate graph references, declared types, and temporal data.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match (self.mode, self.time_format) {
            (GraphMode::Dynamic, None) => {
                return Err(ValidationError(
                    "dynamic graphs require a time_format".into(),
                ));
            }
            (GraphMode::Static, Some(_)) => {
                return Err(ValidationError(
                    "static graphs must not declare a time_format".into(),
                ));
            }
            _ => {}
        }

        let mut definitions = HashMap::new();
        let mut class_modes = HashMap::new();
        for definition in &self.attributes {
            if definition.id.is_empty() {
                return Err(ValidationError(
                    "attribute definition ids cannot be empty".into(),
                ));
            }
            validate_xml_text(&definition.id, "attribute id")?;
            if definition.title.is_empty() {
                return Err(ValidationError(format!(
                    "attribute '{}' has an empty title",
                    definition.id
                )));
            }
            validate_xml_text(&definition.title, "attribute title")?;
            if definition.mode == AttributeMode::Dynamic && self.mode != GraphMode::Dynamic {
                return Err(ValidationError(format!(
                    "dynamic attribute '{}' requires a dynamic graph",
                    definition.id
                )));
            }
            if let Some(existing_mode) = class_modes.insert(definition.class, definition.mode) {
                if existing_mode != definition.mode {
                    return Err(ValidationError(format!(
                        "{:?} attributes must use one mode per class",
                        definition.class
                    )));
                }
            }
            let key = (definition.class, definition.id.clone());
            if definitions
                .insert(key, (definition.value_type, definition.mode))
                .is_some()
            {
                return Err(ValidationError(format!(
                    "duplicate {:?} attribute id '{}'",
                    definition.class, definition.id
                )));
            }
            if let Some(default) = &definition.default {
                validate_value(definition, default)?;
            }
        }

        let mut node_ids = HashSet::new();
        for node in &self.nodes {
            if node.id.is_empty() {
                return Err(ValidationError("node ids cannot be empty".into()));
            }
            validate_xml_text(&node.id, "node id")?;
            if let Some(label) = &node.label {
                validate_xml_text(label, "node label")?;
            }
            if !node_ids.insert(node.id.as_str()) {
                return Err(ValidationError(format!("duplicate node id '{}'", node.id)));
            }
            self.validate_spells(&node.spells, &format!("node '{}'", node.id))?;
            self.validate_values(
                AttributeClass::Node,
                &node.attributes,
                &format!("node '{}'", node.id),
                &definitions,
            )?;
        }

        let mut edge_ids = HashSet::new();
        for edge in &self.edges {
            if edge.id.is_empty() {
                return Err(ValidationError("edge ids cannot be empty".into()));
            }
            validate_xml_text(&edge.id, "edge id")?;
            validate_xml_text(&edge.source, "edge source")?;
            validate_xml_text(&edge.target, "edge target")?;
            if let Some(label) = &edge.label {
                validate_xml_text(label, "edge label")?;
            }
            if !edge_ids.insert(edge.id.as_str()) {
                return Err(ValidationError(format!("duplicate edge id '{}'", edge.id)));
            }
            if !node_ids.contains(edge.source.as_str()) {
                return Err(ValidationError(format!(
                    "edge '{}' references missing source node '{}'",
                    edge.id, edge.source
                )));
            }
            if !node_ids.contains(edge.target.as_str()) {
                return Err(ValidationError(format!(
                    "edge '{}' references missing target node '{}'",
                    edge.id, edge.target
                )));
            }
            self.validate_spells(&edge.spells, &format!("edge '{}'", edge.id))?;
            self.validate_values(
                AttributeClass::Edge,
                &edge.attributes,
                &format!("edge '{}'", edge.id),
                &definitions,
            )?;
        }

        Ok(())
    }

    fn validate_values(
        &self,
        class: AttributeClass,
        values: &[TimedAttributeValue],
        owner: &str,
        definitions: &HashMap<(AttributeClass, String), (ValueType, AttributeMode)>,
    ) -> Result<(), ValidationError> {
        let mut seen: HashMap<&str, Vec<&TimedAttributeValue>> = HashMap::new();
        for timed in values {
            let Some((value_type, mode)) = definitions.get(&(class, timed.attribute_id.clone()))
            else {
                return Err(ValidationError(format!(
                    "{owner} references undeclared {:?} attribute '{}'",
                    class, timed.attribute_id
                )));
            };
            if *value_type != timed.value.value_type() {
                return Err(ValidationError(format!(
                    "{owner} value for '{}' is {:?}, expected {:?}",
                    timed.attribute_id,
                    timed.value.value_type(),
                    value_type
                )));
            }
            let lexical_value = timed.value.lexical_value()?;
            validate_xml_text(&lexical_value, "attribute value")?;
            if let Some(interval) = &timed.interval {
                if *mode != AttributeMode::Dynamic {
                    return Err(ValidationError(format!(
                        "{owner} gives a time interval to static attribute '{}'",
                        timed.attribute_id
                    )));
                }
                self.validate_interval(interval, owner)?;
            }
            if let Some(previous_values) = seen.get(timed.attribute_id.as_str()) {
                if *mode != AttributeMode::Dynamic {
                    return Err(ValidationError(format!(
                        "{owner} has duplicate value for static attribute '{}'",
                        timed.attribute_id
                    )));
                }
                let Some(interval) = &timed.interval else {
                    return Err(ValidationError(format!(
                        "{owner} has multiple values for '{}' but one has no interval",
                        timed.attribute_id
                    )));
                };
                for previous in previous_values {
                    let Some(previous_interval) = &previous.interval else {
                        return Err(ValidationError(format!(
                            "{owner} has multiple values for '{}' but one has no interval",
                            timed.attribute_id
                        )));
                    };
                    if self.intervals_overlap(previous_interval, interval)? {
                        return Err(ValidationError(format!(
                            "{owner} has overlapping intervals for attribute '{}'",
                            timed.attribute_id
                        )));
                    }
                }
            }
            seen.entry(timed.attribute_id.as_str())
                .or_default()
                .push(timed);
        }
        Ok(())
    }

    fn intervals_overlap(
        &self,
        left: &TimeInterval,
        right: &TimeInterval,
    ) -> Result<bool, ValidationError> {
        let time_format = self
            .time_format
            .ok_or_else(|| ValidationError("time intervals require a time_format".into()))?;
        let left_before_right = interval_precedes(
            left.end.as_deref(),
            left.end_open,
            right.start.as_deref(),
            right.start_open,
            time_format,
        )?;
        let right_before_left = interval_precedes(
            right.end.as_deref(),
            right.end_open,
            left.start.as_deref(),
            left.start_open,
            time_format,
        )?;
        Ok(!left_before_right && !right_before_left)
    }

    fn validate_spells(&self, spells: &[TimeInterval], owner: &str) -> Result<(), ValidationError> {
        if !spells.is_empty() && self.mode != GraphMode::Dynamic {
            return Err(ValidationError(format!(
                "{owner} has spells in a static graph"
            )));
        }
        for spell in spells {
            self.validate_interval(spell, owner)?;
        }
        Ok(())
    }

    fn validate_interval(
        &self,
        interval: &TimeInterval,
        owner: &str,
    ) -> Result<(), ValidationError> {
        let Some(time_format) = self.time_format else {
            return Err(ValidationError(format!(
                "{owner} has a time interval without a graph time_format"
            )));
        };
        if interval.start_open && interval.start.is_none() {
            return Err(ValidationError(format!(
                "{owner} has start_open without a start value"
            )));
        }
        if interval.end_open && interval.end.is_none() {
            return Err(ValidationError(format!(
                "{owner} has end_open without an end value"
            )));
        }
        let start = interval
            .start
            .as_deref()
            .map(|value| parse_time(value, time_format))
            .transpose()?;
        let end = interval
            .end
            .as_deref()
            .map(|value| parse_time(value, time_format))
            .transpose()?;
        if let (Some(start), Some(end)) = (start, end) {
            match start.compare(end) {
                Some(Ordering::Greater) => {
                    return Err(ValidationError(format!(
                        "{owner} interval starts after it ends"
                    )));
                }
                Some(Ordering::Equal) if interval.start_open || interval.end_open => {
                    return Err(ValidationError(format!(
                        "{owner} has an empty open interval"
                    )));
                }
                None => {
                    return Err(ValidationError(format!(
                        "{owner} interval endpoints cannot be compared"
                    )));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Write validated GEXF 1.3 XML to any standard-library writer.
    pub fn write_to<W: Write>(&self, mut output: W) -> Result<(), GexfError> {
        self.validate()?;

        writeln!(output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
        writeln!(output, "<gexf xmlns=\"{GEXF_NAMESPACE}\" version=\"1.3\">")?;
        write!(
            output,
            "  <graph mode=\"{}\" defaultedgetype=\"{}\"",
            match self.mode {
                GraphMode::Static => "static",
                GraphMode::Dynamic => "dynamic",
            },
            self.default_edge_type.as_gexf()
        )?;
        if let Some(time_format) = self.time_format {
            write!(output, " timeformat=\"{}\"", time_format.as_gexf())?;
        }
        writeln!(output, ">")?;

        self.write_attribute_definitions(&mut output)?;
        if !self.nodes.is_empty() {
            writeln!(output, "    <nodes>")?;
            for node in &self.nodes {
                self.write_node(&mut output, node)?;
            }
            writeln!(output, "    </nodes>")?;
        }
        if !self.edges.is_empty() {
            writeln!(output, "    <edges>")?;
            for edge in &self.edges {
                self.write_edge(&mut output, edge)?;
            }
            writeln!(output, "    </edges>")?;
        }
        writeln!(output, "  </graph>")?;
        writeln!(output, "</gexf>")?;
        Ok(())
    }

    fn write_attribute_definitions<W: Write>(&self, output: &mut W) -> io::Result<()> {
        for class in [AttributeClass::Node, AttributeClass::Edge] {
            let definitions: Vec<_> = self
                .attributes
                .iter()
                .filter(|item| item.class == class)
                .collect();
            if definitions.is_empty() {
                continue;
            }
            let mode = definitions[0].mode;
            writeln!(
                output,
                "    <attributes class=\"{}\" mode=\"{}\">",
                class.as_gexf(),
                mode.as_gexf()
            )?;
            for definition in definitions {
                writeln!(
                    output,
                    "      <attribute id=\"{}\" title=\"{}\" type=\"{}\">",
                    escape_xml(&definition.id),
                    escape_xml(&definition.title),
                    definition.value_type.as_gexf()
                )?;
                if let Some(default) = &definition.default {
                    let value = default
                        .lexical_value()
                        .expect("graph validation checked default values");
                    writeln!(output, "        <default>{}</default>", escape_xml(&value))?;
                }
                writeln!(output, "      </attribute>")?;
            }
            writeln!(output, "    </attributes>")?;
        }
        Ok(())
    }

    fn write_node<W: Write>(&self, output: &mut W, node: &Node) -> Result<(), GexfError> {
        write!(output, "      <node id=\"{}\"", escape_xml(&node.id))?;
        if let Some(label) = &node.label {
            write!(output, " label=\"{}\"", escape_xml(label))?;
        }
        if node.attributes.is_empty() && node.spells.is_empty() {
            writeln!(output, "/>")?;
            return Ok(());
        }
        writeln!(output, ">")?;
        self.write_spells(output, &node.spells)?;
        self.write_attribute_values(output, &node.attributes)?;
        writeln!(output, "      </node>")?;
        Ok(())
    }

    fn write_edge<W: Write>(&self, output: &mut W, edge: &Edge) -> Result<(), GexfError> {
        write!(
            output,
            "      <edge id=\"{}\" source=\"{}\" target=\"{}\"",
            escape_xml(&edge.id),
            escape_xml(&edge.source),
            escape_xml(&edge.target)
        )?;
        if let Some(label) = &edge.label {
            write!(output, " label=\"{}\"", escape_xml(label))?;
        }
        if edge.attributes.is_empty() && edge.spells.is_empty() {
            writeln!(output, "/>")?;
            return Ok(());
        }
        writeln!(output, ">")?;
        self.write_spells(output, &edge.spells)?;
        self.write_attribute_values(output, &edge.attributes)?;
        writeln!(output, "      </edge>")?;
        Ok(())
    }

    fn write_spells<W: Write>(
        &self,
        output: &mut W,
        spells: &[TimeInterval],
    ) -> Result<(), GexfError> {
        if spells.is_empty() {
            return Ok(());
        }
        writeln!(output, "        <spells>")?;
        for spell in spells {
            write!(output, "          <spell")?;
            write_interval_attributes(output, spell)?;
            writeln!(output, "/>")?;
        }
        writeln!(output, "        </spells>")?;
        Ok(())
    }

    fn write_attribute_values<W: Write>(
        &self,
        output: &mut W,
        values: &[TimedAttributeValue],
    ) -> Result<(), GexfError> {
        if values.is_empty() {
            return Ok(());
        }
        writeln!(output, "        <attvalues>")?;
        for timed in values {
            let value = timed.value.lexical_value()?;
            write!(
                output,
                "          <attvalue for=\"{}\" value=\"{}\"",
                escape_xml(&timed.attribute_id),
                escape_xml(&value)
            )?;
            if let Some(interval) = &timed.interval {
                write_interval_attributes(output, interval)?;
            }
            writeln!(output, "/>")?;
        }
        writeln!(output, "        </attvalues>")?;
        Ok(())
    }
}

fn write_interval_attributes<W: Write>(output: &mut W, interval: &TimeInterval) -> io::Result<()> {
    if let Some(start) = &interval.start {
        let attribute = if interval.start_open {
            "startopen"
        } else {
            "start"
        };
        write!(output, " {attribute}=\"{}\"", escape_xml(start))?;
    }
    if let Some(end) = &interval.end {
        let attribute = if interval.end_open { "endopen" } else { "end" };
        write!(output, " {attribute}=\"{}\"", escape_xml(end))?;
    }
    Ok(())
}

fn validate_value(
    definition: &AttributeDefinition,
    value: &AttributeValue,
) -> Result<(), ValidationError> {
    if definition.value_type != value.value_type() {
        return Err(ValidationError(format!(
            "default for '{}' is {:?}, expected {:?}",
            definition.id,
            value.value_type(),
            definition.value_type
        )));
    }
    let lexical = value.lexical_value()?;
    validate_xml_text(&lexical, "attribute default")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum ParsedTime {
    Integer(i128),
    Number(f64),
}

impl ParsedTime {
    fn compare(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => Some(left.cmp(&right)),
            (Self::Number(left), Self::Number(right)) => left.partial_cmp(&right),
            _ => None,
        }
    }
}

fn parse_time(value: &str, format: TimeFormat) -> Result<ParsedTime, ValidationError> {
    let parsed = match format {
        TimeFormat::Integer => value.parse::<i128>().ok().map(ParsedTime::Integer),
        TimeFormat::Double => value
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .map(ParsedTime::Number),
        TimeFormat::Date => parse_date(value).map(|days| ParsedTime::Integer(i128::from(days))),
        TimeFormat::DateTime => parse_datetime(value).map(ParsedTime::Integer),
    };
    parsed.ok_or_else(|| {
        ValidationError(format!(
            "time value '{value}' is not valid for format {:?}",
            format
        ))
    })
}

fn interval_precedes(
    left_end: Option<&str>,
    left_end_open: bool,
    right_start: Option<&str>,
    right_start_open: bool,
    time_format: TimeFormat,
) -> Result<bool, ValidationError> {
    match (left_end, right_start) {
        (Some(left_end), Some(right_start)) => {
            let left_end = parse_time(left_end, time_format)?;
            let right_start = parse_time(right_start, time_format)?;
            Ok(match left_end.compare(right_start) {
                Some(Ordering::Less) => true,
                Some(Ordering::Equal) => left_end_open || right_start_open,
                Some(Ordering::Greater) => false,
                None => {
                    return Err(ValidationError(
                        "interval endpoints cannot be compared".into(),
                    ))
                }
            })
        }
        _ => Ok(false),
    }
}

fn parse_date(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
        || bytes[4] != b'-'
        || bytes[7] != b'-'
    {
        return None;
    }
    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<u32>().ok()?;
    let day = value[8..10].parse::<u32>().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > month_days {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era
}

fn parse_datetime(value: &str) -> Option<i128> {
    let (date, time_and_zone) = value.split_once('T')?;
    let days = parse_date(date)?;

    let (time, offset_seconds) = if let Some(time) = time_and_zone.strip_suffix('Z') {
        (time, 0_i64)
    } else if let Some(index) = time_and_zone
        .char_indices()
        .skip(8)
        .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index))
    {
        let (time, zone) = time_and_zone.split_at(index);
        let zone_bytes = zone.as_bytes();
        if zone_bytes.len() != 6
            || !zone_bytes[1..3].iter().all(u8::is_ascii_digit)
            || zone_bytes[3] != b':'
            || !zone_bytes[4..6].iter().all(u8::is_ascii_digit)
        {
            return None;
        }
        let hours = i64::from(zone_bytes[1] - b'0') * 10 + i64::from(zone_bytes[2] - b'0');
        let minutes = i64::from(zone_bytes[4] - b'0') * 10 + i64::from(zone_bytes[5] - b'0');
        if hours > 14 || minutes > 59 || (hours == 14 && minutes != 0) {
            return None;
        }
        let magnitude = hours * 3_600 + minutes * 60;
        let offset = if zone_bytes[0] == b'+' {
            magnitude
        } else {
            -magnitude
        };
        (time, offset)
    } else {
        (time_and_zone, 0_i64)
    };

    let time_bytes = time.as_bytes();
    if time_bytes.len() < 8
        || time_bytes[2] != b':'
        || time_bytes[5] != b':'
        || !time_bytes[..2].iter().all(u8::is_ascii_digit)
        || !time_bytes[3..5].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let seconds = &time_bytes[6..];
    if seconds.len() < 2 || !seconds[..2].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let fractional_nanos = if seconds.len() == 2 {
        0_i128
    } else {
        if seconds.len() < 4
            || seconds.len() > 12
            || seconds[2] != b'.'
            || !seconds[3..].iter().all(u8::is_ascii_digit)
        {
            return None;
        }
        let fraction = std::str::from_utf8(&seconds[3..])
            .ok()?
            .parse::<i128>()
            .ok()?;
        fraction * 10_i128.pow((9 - (seconds.len() - 3)) as u32)
    };
    let hour = u32::from(time_bytes[0] - b'0') * 10 + u32::from(time_bytes[1] - b'0');
    let minute = u32::from(time_bytes[3] - b'0') * 10 + u32::from(time_bytes[4] - b'0');
    let second = u32::from(seconds[0] - b'0') * 10 + u32::from(seconds[1] - b'0');
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let timestamp_seconds = i128::from(days) * 86_400
        + i128::from(hour) * 3_600
        + i128::from(minute) * 60
        + i128::from(second)
        - i128::from(offset_seconds);
    let timestamp = timestamp_seconds * 1_000_000_000 + fractional_nanos;
    Some(timestamp)
}

fn validate_xml_text(value: &str, context: &str) -> Result<(), ValidationError> {
    if value.chars().any(|character| {
        let code = character as u32;
        !matches!(code, 0x9 | 0xA | 0xD)
            && !(0x20..=0xD7FF).contains(&code)
            && !(0xE000..=0xFFFD).contains(&code)
            && !(0x10000..=0x10FFFF).contains(&code)
    }) {
        return Err(ValidationError(format!(
            "{context} contains a character that is not legal in XML 1.0"
        )));
    }
    Ok(())
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\t' => escaped.push_str("&#x9;"),
            '\n' => escaped.push_str("&#xA;"),
            '\r' => escaped.push_str("&#xD;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> Graph {
        serde_json::from_str(include_str!("../samples/typed_dynamic_graph.json"))
            .expect("synthetic sample should deserialize")
    }

    fn write(graph: &Graph) -> String {
        let mut xml = Vec::new();
        graph.write_to(&mut xml).expect("sample graph should write");
        String::from_utf8(xml).expect("writer emits UTF-8")
    }

    #[test]
    fn sample_serializes_typed_dynamic_graph() {
        let graph = sample_graph();
        graph.validate().expect("sample should validate");
        let xml = write(&graph);

        assert!(xml.contains(&format!(
            "<gexf xmlns=\"{GEXF_NAMESPACE}\" version=\"1.3\">"
        )));
        assert!(xml.contains("mode=\"dynamic\" defaultedgetype=\"directed\" timeformat=\"date\""));
        assert!(xml.contains("<attribute id=\"rank\" title=\"Rank\" type=\"integer\">"));
        assert!(xml.contains(
            "<attvalue for=\"rank\" value=\"7\" start=\"2026-01-01\" end=\"2026-06-30\"/>"
        ));
        assert!(xml.contains("<spells>"));
        assert!(xml.contains("<spell start=\"2026-01-01\" end=\"2026-12-31\"/>"));
        assert!(xml.contains(
            "<attvalue for=\"weight\" value=\"0.75\" start=\"2026-03-01\" endopen=\"2026-12-31\"/>"
        ));
        assert!(xml.contains("<spell start=\"2026-03-01\" endopen=\"2026-12-31\"/>"));
        assert!(!xml.contains("endopen=\"true\""));
        assert!(xml.contains("type=\"double\""));
    }

    #[test]
    fn serializes_multiple_non_overlapping_values_for_a_dynamic_attribute() {
        let mut graph = sample_graph();
        graph.nodes[0].attributes.push(TimedAttributeValue {
            attribute_id: "rank".into(),
            value: AttributeValue::Integer(9),
            interval: Some(TimeInterval {
                start: Some("2026-07-01".into()),
                end: Some("2026-12-31".into()),
                ..TimeInterval::default()
            }),
        });

        let xml = write(&graph);
        assert!(xml.contains(
            "<attvalue for=\"rank\" value=\"9\" start=\"2026-07-01\" end=\"2026-12-31\"/>"
        ));
    }

    #[test]
    fn rejects_overlapping_dynamic_values_and_mixed_class_modes() {
        let mut graph = sample_graph();
        graph.nodes[0].attributes.push(TimedAttributeValue {
            attribute_id: "rank".into(),
            value: AttributeValue::Integer(9),
            interval: Some(TimeInterval {
                start: Some("2026-06-01".into()),
                end: Some("2026-12-31".into()),
                ..TimeInterval::default()
            }),
        });
        assert!(graph
            .validate()
            .unwrap_err()
            .to_string()
            .contains("overlapping intervals"));

        graph.nodes[0].attributes.pop();
        graph.attributes[0].mode = AttributeMode::Static;
        assert!(graph
            .validate()
            .unwrap_err()
            .to_string()
            .contains("one mode per class"));
    }

    #[test]
    fn escapes_xml_text_and_attributes() {
        let mut graph = sample_graph();
        graph.nodes[0].label = Some("A < B & \"C\"".into());
        graph.nodes[0].attributes[0].value = AttributeValue::String("R&D <North>".into());
        let xml = write(&graph);

        assert!(xml.contains("label=\"A &lt; B &amp; &quot;C&quot;\""));
        assert!(xml.contains("value=\"R&amp;D &lt;North&gt;\""));
    }

    #[test]
    fn rejects_dynamic_data_on_static_graph() {
        let mut graph = sample_graph();
        graph.mode = GraphMode::Static;
        graph.time_format = None;

        let error = graph
            .validate()
            .expect_err("dynamic definitions are invalid");
        assert!(error.to_string().contains("dynamic attribute"));
    }

    #[test]
    fn rejects_undeclared_or_wrongly_typed_values() {
        let mut graph = sample_graph();
        graph.nodes[0].attributes[0].value = AttributeValue::Integer(7);

        let error = graph.validate().expect_err("type mismatch is invalid");
        assert!(error.to_string().contains("expected String"));
    }

    #[test]
    fn rejects_edges_that_reference_missing_nodes() {
        let mut graph = sample_graph();
        graph.edges[0].target = "missing-node".into();

        let error = graph.validate().expect_err("dangling edge is invalid");
        assert!(error.to_string().contains("missing target node"));
    }

    #[test]
    fn rejects_invalid_and_empty_open_intervals() {
        let mut graph = sample_graph();
        graph.edges[0].spells[0] = TimeInterval {
            start: Some("2026-12-31".into()),
            end: Some("2026-01-01".into()),
            ..TimeInterval::default()
        };
        assert!(graph
            .validate()
            .unwrap_err()
            .to_string()
            .contains("starts after"));

        graph.edges[0].spells[0] = TimeInterval {
            start: Some("2026-01-01".into()),
            end: Some("2026-01-01".into()),
            start_open: true,
            ..TimeInterval::default()
        };
        assert!(graph
            .validate()
            .unwrap_err()
            .to_string()
            .contains("empty open interval"));
    }

    #[test]
    fn accepts_timezone_aware_datetime_intervals() {
        let graph = Graph {
            mode: GraphMode::Dynamic,
            time_format: Some(TimeFormat::DateTime),
            default_edge_type: DefaultEdgeType::Directed,
            attributes: Vec::new(),
            nodes: vec![Node {
                id: "synthetic".into(),
                label: None,
                attributes: Vec::new(),
                spells: vec![TimeInterval {
                    start: Some("2026-01-01T02:00:00+02:00".into()),
                    end: Some("2026-01-01T01:00:00Z".into()),
                    ..TimeInterval::default()
                }],
            }],
            edges: Vec::new(),
        };

        assert!(graph.validate().is_ok());
    }

    #[test]
    fn compares_datetime_intervals_at_nanosecond_precision() {
        assert!(interval_precedes(
            Some("2026-01-01T00:00:00.000000001Z"),
            false,
            Some("2026-01-01T00:00:00.000000002Z"),
            false,
            TimeFormat::DateTime,
        )
        .expect("valid date-time endpoints should compare"));
    }

    #[test]
    fn rejects_non_ascii_malformed_dates_without_panicking() {
        let mut graph = sample_graph();
        graph.nodes[0].spells[0].start = Some("202é-01-01".into());

        assert!(graph.validate().is_err());
    }

    #[test]
    fn compares_large_integer_timestamps_without_float_rounding() {
        let mut graph = sample_graph();
        graph.time_format = Some(TimeFormat::Integer);
        graph.nodes[0].spells[0] = TimeInterval {
            start: Some("9007199254740993".into()),
            end: Some("9007199254740992".into()),
            ..TimeInterval::default()
        };

        assert!(graph
            .validate()
            .unwrap_err()
            .to_string()
            .contains("starts after"));
    }

    #[test]
    fn rejects_xml_control_characters() {
        let mut graph = sample_graph();
        graph.nodes[0].label = Some("bad\u{1}label".into());

        assert!(graph
            .validate()
            .unwrap_err()
            .to_string()
            .contains("XML 1.0"));
    }
}
