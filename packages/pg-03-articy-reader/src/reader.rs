// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{json, *};
use serde_json::{Map, Number, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

/// Convert a native single-file JSON export. No filesystem, network or script execution.
pub fn read_export<R: Read>(reader: R, options: &ImportOptions) -> Result<DialogueGraph> {
    if [
        options.max_input_bytes,
        options.max_nodes,
        options.max_pins,
        options.max_edges,
        options.max_variables,
        options.max_hierarchy_entries,
        options.max_warnings,
        options.max_depth,
        options.max_identifier_bytes,
    ]
    .contains(&0)
    {
        return Err(Error::invalid("options", "all limits must be positive"));
    }
    let mut root = object(json::decode(reader, options)?, "$")?;
    let packages = required_array(root.remove("Packages"), "Packages")?;
    let definitions = optional_array(root.remove("ObjectDefinitions"), "ObjectDefinitions")?;
    let variables = optional_array(root.remove("GlobalVariables"), "GlobalVariables")?;
    let hierarchy = root.remove("Hierarchy");
    let classes = definitions_map(&definitions, options)?;
    let text_mode = text_mode(&root)?;
    let mut graph = DialogueGraph {
        version: 1,
        text_mode,
        metadata: root,
        definitions,
        packages: Vec::new(),
        nodes: Vec::new(),
        pins: Vec::new(),
        edges: Vec::new(),
        choices: Vec::new(),
        variable_namespaces: Vec::new(),
        variables: Vec::new(),
        hierarchy: Vec::new(),
        warnings: Vec::new(),
    };
    if text_mode == TextMode::LocalizationKeys {
        warn(
            &mut graph,
            options,
            WarningKind::LocalizationKeys,
            None,
            None,
        )?;
    }
    let mut ids = BTreeSet::new();
    for (package_index, package) in packages.into_iter().enumerate() {
        let path = format!("Packages[{package_index}]");
        let mut package = object(package, &path)?;
        let name = required_string(package.get("Name"), &format!("{path}.Name"))?.to_owned();
        let models = required_array(package.remove("Models"), &format!("{path}.Models"))?;
        let mut node_ids = Vec::new();
        for (model_index, model) in models.into_iter().enumerate() {
            if graph.nodes.len() >= options.max_nodes {
                return Err(Error::Limit("nodes"));
            }
            let path = format!("{path}.Models[{model_index}]");
            let mut model = object(model, &path)?;
            let source_type = identifier(model.get("Type"), &format!("{path}.Type"), options)?;
            let mut properties = object(
                model
                    .remove("Properties")
                    .ok_or_else(|| Error::invalid(&path, "missing Properties"))?,
                &format!("{path}.Properties"),
            )?;
            let id = identifier(
                properties.get("Id"),
                &format!("{path}.Properties.Id"),
                options,
            )?;
            unique_id(&id, &mut ids, &path)?;
            let kind = node_kind(&source_type, &classes, options.max_depth)?;
            if kind == NodeKind::Other
                && !classes.contains_key(&source_type)
                && !known_class(&source_type)
            {
                warn(
                    &mut graph,
                    options,
                    WarningKind::UnknownType,
                    Some(&id),
                    Some(&source_type),
                )?;
            }
            let input = optional_array(
                properties.remove("InputPins"),
                &format!("{path}.Properties.InputPins"),
            )?;
            let output = optional_array(
                properties.remove("OutputPins"),
                &format!("{path}.Properties.OutputPins"),
            )?;
            let input_pins = pins(
                input,
                PinDirection::Input,
                &id,
                &path,
                &mut graph,
                &mut ids,
                options,
            )?;
            let output_pins = pins(
                output,
                PinDirection::Output,
                &id,
                &path,
                &mut graph,
                &mut ids,
                options,
            )?;
            let parent = reference(
                properties.get("Parent"),
                &format!("{path}.Properties.Parent"),
                options,
            )?;
            let speaker = reference(
                properties.get("Speaker"),
                &format!("{path}.Properties.Speaker"),
                options,
            )?;
            let role = match kind {
                NodeKind::Condition => Some(ScriptRole::Condition),
                NodeKind::Instruction => Some(ScriptRole::Instruction),
                _ => None,
            };
            let expression = optional_string(
                properties.get("Expression"),
                &format!("{path}.Properties.Expression"),
            )?;
            let script = role.and_then(|role| expression.map(|text| script(role, text)));
            if role.is_some() && expression.is_none() {
                warn(
                    &mut graph,
                    options,
                    WarningKind::MissingScript,
                    Some(&id),
                    None,
                )?;
            }
            if kind == NodeKind::Jump {
                if let Some(target) = reference(
                    properties.get("Target"),
                    &format!("{path}.Properties.Target"),
                    options,
                )? {
                    let target_pin = reference(
                        properties.get("TargetPin"),
                        &format!("{path}.Properties.TargetPin"),
                        options,
                    )?;
                    edge(
                        &mut graph,
                        options,
                        Edge {
                            id: String::new(),
                            kind: EdgeKind::Jump,
                            source: id.clone(),
                            source_pin: None,
                            target,
                            target_pin,
                            index: 0,
                            label: None,
                            properties: Map::new(),
                        },
                    )?;
                }
            }
            let node = Node {
                id: id.clone(),
                package: package_index,
                source_type,
                kind,
                parent,
                technical_name: optional_string(
                    properties.get("TechnicalName"),
                    &format!("{path}.Properties.TechnicalName"),
                )?
                .map(str::to_owned),
                display_name: optional_string(
                    properties.get("DisplayName"),
                    &format!("{path}.Properties.DisplayName"),
                )?
                .map(str::to_owned),
                text: optional_string(properties.get("Text"), &format!("{path}.Properties.Text"))?
                    .map(str::to_owned),
                menu_text: optional_string(
                    properties.get("MenuText"),
                    &format!("{path}.Properties.MenuText"),
                )?
                .map(str::to_owned),
                speaker,
                script,
                input_pins,
                output_pins,
                template: model.remove("Template"),
                properties,
                metadata: model,
            };
            node_ids.push(id);
            graph.nodes.push(node);
        }
        graph.packages.push(Package {
            index: package_index,
            name,
            metadata: package,
            node_ids,
        });
    }
    parse_variables(variables, &mut graph, options)?;
    if let Some(hierarchy) = hierarchy.filter(|value| !value.is_null()) {
        parse_hierarchy(
            hierarchy,
            None,
            0,
            0,
            &mut graph,
            options,
            &mut BTreeSet::new(),
        )?;
    }
    validate(&mut graph, options)?;
    Ok(graph)
}

fn object(value: Value, path: &str) -> Result<Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(Error::invalid(path, "expected object")),
    }
}
fn required_array(value: Option<Value>, path: &str) -> Result<Vec<Value>> {
    match value {
        Some(Value::Array(values)) => Ok(values),
        _ => Err(Error::invalid(path, "expected array")),
    }
}
fn optional_array(value: Option<Value>, path: &str) -> Result<Vec<Value>> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        value => required_array(value, path),
    }
}
fn required_string<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a str> {
    match value {
        Some(Value::String(value)) => Ok(value),
        _ => Err(Error::invalid(path, "expected string")),
    }
}
fn optional_string<'a>(value: Option<&'a Value>, path: &str) -> Result<Option<&'a str>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        _ => required_string(value, path).map(Some),
    }
}
fn identifier(value: Option<&Value>, path: &str, options: &ImportOptions) -> Result<String> {
    let value = required_string(value, path)?;
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(Error::invalid(
            path,
            "identifier must be nonempty, trimmed and control-free",
        ));
    }
    if value.len() > options.max_identifier_bytes {
        return Err(Error::Limit("identifier bytes"));
    }
    Ok(value.to_owned())
}
fn is_null_id(id: &str) -> bool {
    id.strip_prefix("0x")
        .or_else(|| id.strip_prefix("0X"))
        .is_some_and(|hex| !hex.is_empty() && hex.bytes().all(|b| b == b'0'))
}
fn reference(value: Option<&Value>, path: &str, options: &ImportOptions) -> Result<Option<String>> {
    if matches!(value, None | Some(Value::Null)) {
        return Ok(None);
    }
    let id = identifier(value, path, options)?;
    Ok((!is_null_id(&id)).then_some(id))
}
fn unique_id(id: &str, ids: &mut BTreeSet<String>, path: &str) -> Result<()> {
    if is_null_id(id) || !ids.insert(id.into()) {
        return Err(Error::invalid(
            path,
            "object and pin IDs must be nonzero and globally unique",
        ));
    }
    Ok(())
}
fn script(role: ScriptRole, text: &str) -> Script {
    Script {
        language: "articy_script".into(),
        role,
        text: text.into(),
    }
}

fn text_mode(root: &Map<String, Value>) -> Result<TextMode> {
    let Some(settings) = root.get("Settings") else {
        return Ok(TextMode::Literal);
    };
    let settings = settings
        .as_object()
        .ok_or_else(|| Error::invalid("Settings", "expected object"))?;
    match settings.get("set_Localization") {
        None | Some(Value::Bool(false)) => Ok(TextMode::Literal),
        Some(Value::Bool(true)) => Ok(TextMode::LocalizationKeys),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("true") => {
            Ok(TextMode::LocalizationKeys)
        }
        Some(Value::String(value)) if value.eq_ignore_ascii_case("false") => Ok(TextMode::Literal),
        _ => Err(Error::invalid(
            "Settings.set_Localization",
            "expected boolean or True/False string",
        )),
    }
}

struct Definition {
    class: String,
    parent: Option<String>,
}
fn definitions_map(
    values: &[Value],
    options: &ImportOptions,
) -> Result<BTreeMap<String, Definition>> {
    let mut result = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        let path = format!("ObjectDefinitions[{index}]");
        let value = value
            .as_object()
            .ok_or_else(|| Error::invalid(&path, "expected object"))?;
        let name = identifier(value.get("Type"), &format!("{path}.Type"), options)?;
        let class = identifier(value.get("Class"), &format!("{path}.Class"), options)?;
        let parent = if value
            .get("InheritsFrom")
            .is_some_and(|value| !value.is_null())
        {
            Some(identifier(
                value.get("InheritsFrom"),
                &format!("{path}.InheritsFrom"),
                options,
            )?)
        } else {
            None
        };
        if result.insert(name, Definition { class, parent }).is_some() {
            return Err(Error::invalid(path, "duplicate type definition"));
        }
    }
    // Validate all definitions, even if the export contains no instances of them.
    for name in result.keys() {
        node_kind(name, &result, options.max_depth)?;
    }
    Ok(result)
}
fn known_class(class: &str) -> bool {
    matches!(
        class,
        "ArticyObject"
            | "Primitive"
            | "Enum"
            | "FlowFragment"
            | "Dialogue"
            | "DialogueFragment"
            | "Hub"
            | "Jump"
            | "Condition"
            | "Instruction"
            | "Entity"
            | "UserFolder"
            | "Comment"
            | "Asset"
            | "Location"
            | "Spot"
            | "Zone"
            | "Path"
            | "Link"
            | "LocationText"
            | "LocationImage"
            | "Document"
            | "TextObject"
    )
}
fn class_kind(class: &str) -> NodeKind {
    match class {
        "FlowFragment" => NodeKind::FlowFragment,
        "Dialogue" => NodeKind::Dialogue,
        "DialogueFragment" => NodeKind::DialogueFragment,
        "Hub" => NodeKind::Hub,
        "Jump" => NodeKind::Jump,
        "Condition" => NodeKind::Condition,
        "Instruction" => NodeKind::Instruction,
        "Entity" => NodeKind::Entity,
        "UserFolder" => NodeKind::UserFolder,
        "Comment" => NodeKind::Comment,
        "Asset" => NodeKind::Asset,
        _ => NodeKind::Other,
    }
}
fn node_kind(name: &str, classes: &BTreeMap<String, Definition>, limit: usize) -> Result<NodeKind> {
    let mut current = name;
    let mut visited = BTreeSet::new();
    let mut kind = class_kind(name);
    for _ in 0..limit {
        if !visited.insert(current) {
            return Err(Error::invalid(
                "ObjectDefinitions",
                "cyclic type inheritance",
            ));
        }
        let Some(definition) = classes.get(current) else {
            return Ok(kind);
        };
        if kind == NodeKind::Other {
            kind = class_kind(&definition.class);
        }
        let Some(parent) = definition.parent.as_deref() else {
            return Ok(kind);
        };
        current = parent;
        if kind == NodeKind::Other {
            kind = class_kind(current);
        }
    }
    Err(Error::Limit("type inheritance depth"))
}

fn pins(
    values: Vec<Value>,
    direction: PinDirection,
    owner: &str,
    path: &str,
    graph: &mut DialogueGraph,
    ids: &mut BTreeSet<String>,
    options: &ImportOptions,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let direction_name = if direction == PinDirection::Input {
        "InputPins"
    } else {
        "OutputPins"
    };
    for (index, value) in values.into_iter().enumerate() {
        if graph.pins.len() >= options.max_pins {
            return Err(Error::Limit("pins"));
        }
        let path = format!("{path}.Properties.{direction_name}[{index}]");
        let mut properties = object(value, &path)?;
        let id = identifier(properties.get("Id"), &format!("{path}.Id"), options)?;
        unique_id(&id, ids, &path)?;
        let declared_owner =
            identifier(properties.get("Owner"), &format!("{path}.Owner"), options)?;
        if declared_owner != owner {
            return Err(Error::invalid(
                &path,
                "pin Owner disagrees with containing model",
            ));
        }
        let text = optional_string(properties.get("Text"), &format!("{path}.Text"))?;
        let role = if direction == PinDirection::Input {
            ScriptRole::Condition
        } else {
            ScriptRole::Instruction
        };
        let script = text
            .filter(|text| !text.is_empty())
            .map(|text| script(role, text));
        let connections = optional_array(
            properties.remove("Connections"),
            &format!("{path}.Connections"),
        )?;
        for (connection_index, value) in connections.into_iter().enumerate() {
            let path = format!("{path}.Connections[{connection_index}]");
            let properties = object(value, &path)?;
            let target =
                reference(properties.get("Target"), &format!("{path}.Target"), options)?
                    .ok_or_else(|| Error::invalid(&path, "connection Target must be nonzero"))?;
            let target_pin = reference(
                properties.get("TargetPin"),
                &format!("{path}.TargetPin"),
                options,
            )?
            .ok_or_else(|| Error::invalid(&path, "connection TargetPin must be nonzero"))?;
            let label = optional_string(properties.get("Label"), &format!("{path}.Label"))?
                .map(str::to_owned);
            edge(
                graph,
                options,
                Edge {
                    id: String::new(),
                    kind: EdgeKind::Connection,
                    source: owner.into(),
                    source_pin: Some(id.clone()),
                    target,
                    target_pin: Some(target_pin),
                    index: connection_index,
                    label,
                    properties,
                },
            )?;
        }
        result.push(id.clone());
        graph.pins.push(Pin {
            id,
            owner: owner.into(),
            direction,
            index,
            script,
            properties,
        });
    }
    Ok(result)
}
fn edge(graph: &mut DialogueGraph, options: &ImportOptions, mut edge: Edge) -> Result<()> {
    if graph.edges.len() >= options.max_edges {
        return Err(Error::Limit("edges"));
    }
    edge.id = format!("edge-{}", graph.edges.len() + 1);
    graph.edges.push(edge);
    Ok(())
}
fn warn(
    graph: &mut DialogueGraph,
    options: &ImportOptions,
    kind: WarningKind,
    source: Option<&str>,
    reference: Option<&str>,
) -> Result<()> {
    if graph.warnings.len() >= options.max_warnings {
        return Err(Error::Limit("warnings"));
    }
    graph.warnings.push(Warning {
        kind,
        source: source.map(str::to_owned),
        reference: reference.map(str::to_owned),
    });
    Ok(())
}

fn parse_variables(
    values: Vec<Value>,
    graph: &mut DialogueGraph,
    options: &ImportOptions,
) -> Result<()> {
    let mut namespaces = BTreeSet::new();
    for (namespace_index, value) in values.into_iter().enumerate() {
        let path = format!("GlobalVariables[{namespace_index}]");
        let mut metadata = object(value, &path)?;
        let name = identifier(
            metadata.get("Namespace"),
            &format!("{path}.Namespace"),
            options,
        )?;
        if !namespaces.insert(name.clone()) {
            return Err(Error::invalid(&path, "duplicate variable namespace"));
        }
        let variables = required_array(metadata.remove("Variables"), &format!("{path}.Variables"))?;
        let mut names = BTreeSet::new();
        let mut variable_names = Vec::new();
        for (index, value) in variables.into_iter().enumerate() {
            if graph.variables.len() >= options.max_variables {
                return Err(Error::Limit("variables"));
            }
            let path = format!("{path}.Variables[{index}]");
            let mut metadata = object(value, &path)?;
            let variable_name = identifier(
                metadata.get("Variable"),
                &format!("{path}.Variable"),
                options,
            )?;
            if !names.insert(variable_name.clone()) {
                return Err(Error::invalid(
                    &path,
                    "duplicate variable name in namespace",
                ));
            }
            let source_type = identifier(metadata.get("Type"), &format!("{path}.Type"), options)?;
            let raw_value = metadata
                .remove("Value")
                .ok_or_else(|| Error::invalid(&path, "variable has no Value"))?;
            let (kind, value) = variable_value(&source_type, &raw_value, &path)?;
            if kind == VariableKind::Other {
                warn(
                    graph,
                    options,
                    WarningKind::UnknownVariableType,
                    Some(&name),
                    Some(&variable_name),
                )?;
            }
            let description =
                optional_string(metadata.get("Description"), &format!("{path}.Description"))?
                    .map(str::to_owned);
            variable_names.push(variable_name.clone());
            graph.variables.push(Variable {
                namespace: name.clone(),
                name: variable_name,
                source_type,
                kind,
                value,
                raw_value,
                description,
                metadata,
            });
        }
        graph.variable_namespaces.push(VariableNamespace {
            name,
            metadata,
            variables: variable_names,
        });
    }
    Ok(())
}
fn variable_value(kind: &str, value: &Value, path: &str) -> Result<(VariableKind, Value)> {
    let invalid = || Error::invalid(path, "variable Value does not match declared Type");
    match kind.to_ascii_lowercase().as_str() {
        "boolean" | "bool" => {
            let boolean = match value {
                Value::Bool(value) => *value,
                Value::String(value) if value.eq_ignore_ascii_case("true") => true,
                Value::String(value) if value.eq_ignore_ascii_case("false") => false,
                _ => return Err(invalid()),
            };
            Ok((VariableKind::Boolean, Value::Bool(boolean)))
        }
        "integer" | "int" => {
            let integer = match value {
                Value::String(value) => value.parse::<i64>().ok(),
                Value::Number(value) => value.as_i64(),
                _ => None,
            }
            .ok_or_else(invalid)?;
            Ok((VariableKind::Integer, Value::Number(integer.into())))
        }
        "float" | "double" => {
            let float = match value {
                Value::String(value) => value.parse::<f64>().ok(),
                Value::Number(value) => value.as_f64(),
                _ => None,
            }
            .ok_or_else(invalid)?;
            let number = Number::from_f64(float).ok_or_else(invalid)?;
            Ok((VariableKind::Float, Value::Number(number)))
        }
        "string" => match value {
            Value::String(_) => Ok((VariableKind::String, value.clone())),
            _ => Err(invalid()),
        },
        _ => Ok((VariableKind::Other, value.clone())),
    }
}

fn parse_hierarchy(
    value: Value,
    parent: Option<&str>,
    index: usize,
    depth: usize,
    graph: &mut DialogueGraph,
    options: &ImportOptions,
    ids: &mut BTreeSet<String>,
) -> Result<()> {
    if graph.hierarchy.len() >= options.max_hierarchy_entries {
        return Err(Error::Limit("hierarchy entries"));
    }
    if depth >= options.max_depth {
        return Err(Error::Limit("hierarchy depth"));
    }
    let mut properties = object(value, "Hierarchy")?;
    let id = identifier(properties.get("Id"), "Hierarchy.Id", options)?;
    if is_null_id(&id) || !ids.insert(id.clone()) {
        return Err(Error::invalid("Hierarchy", "zero or repeated hierarchy ID"));
    }
    let children = optional_array(properties.remove("Children"), "Hierarchy.Children")?;
    graph.hierarchy.push(HierarchyEntry {
        id: id.clone(),
        parent: parent.map(str::to_owned),
        index,
        depth,
        properties,
    });
    for (index, child) in children.into_iter().enumerate() {
        parse_hierarchy(child, Some(&id), index, depth + 1, graph, options, ids)?;
    }
    Ok(())
}

fn validate(graph: &mut DialogueGraph, options: &ImportOptions) -> Result<()> {
    let nodes: BTreeMap<_, _> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let pins: BTreeMap<_, _> = graph
        .pins
        .iter()
        .enumerate()
        .map(|(index, pin)| (pin.id.clone(), index))
        .collect();
    let mut warnings = Vec::new();
    let mut pending = |kind, source: &str, reference: &str| -> Result<()> {
        if options.strict_references {
            return Err(Error::invalid(
                "references",
                "unresolved or inconsistent model reference",
            ));
        }
        if warnings.len().saturating_add(graph.warnings.len()) >= options.max_warnings {
            return Err(Error::Limit("warnings"));
        }
        warnings.push((kind, source.to_owned(), reference.to_owned()));
        Ok(())
    };
    for entry in &graph.hierarchy {
        if pins.contains_key(&entry.id) {
            return Err(Error::invalid(
                "Hierarchy.Id",
                "hierarchy entry is a pin, not a model",
            ));
        }
        if let Some(index) = nodes.get(&entry.id) {
            let node = &mut graph.nodes[*index];
            if !node.properties.contains_key("Parent") {
                node.parent.clone_from(&entry.parent);
            } else if let Some(parent) = &entry.parent {
                if node.parent.as_deref() != Some(parent.as_str()) {
                    pending(WarningKind::HierarchyParentMismatch, &entry.id, parent)?;
                }
            }
        } else {
            pending(WarningKind::MissingHierarchyObject, &entry.id, &entry.id)?;
        }
    }
    for node in &graph.nodes {
        if let Some(parent) = &node.parent {
            if pins.contains_key(parent) {
                return Err(Error::invalid(
                    "Parent",
                    "parent reference is a pin, not a model",
                ));
            }
            if !nodes.contains_key(parent) {
                pending(WarningKind::MissingParent, &node.id, parent)?;
            }
        }
        if let Some(speaker) = &node.speaker {
            if pins.contains_key(speaker) {
                return Err(Error::invalid(
                    "Speaker",
                    "speaker reference is a pin, not a model",
                ));
            }
            if !nodes.contains_key(speaker) {
                pending(WarningKind::MissingSpeaker, &node.id, speaker)?;
            }
        }
    }
    check_parent_cycles(&graph.nodes, &nodes)?;
    for edge in &graph.edges {
        if pins.contains_key(&edge.target) {
            return Err(Error::invalid(
                "Target",
                "edge target is a pin, not a model",
            ));
        }
        if !nodes.contains_key(&edge.target) {
            pending(WarningKind::MissingNode, &edge.id, &edge.target)?;
        }
        if let Some(target_pin) = &edge.target_pin {
            if let Some(index) = pins.get(target_pin) {
                if graph.pins[*index].owner != edge.target {
                    return Err(Error::invalid(
                        "TargetPin",
                        "target pin is not owned by Target",
                    ));
                }
            } else {
                if nodes.contains_key(target_pin) {
                    return Err(Error::invalid("TargetPin", "pin reference is a model ID"));
                }
                pending(WarningKind::MissingPin, &edge.id, target_pin)?;
            }
        }
    }
    for (kind, source, reference) in warnings {
        if options.strict_references {
            return Err(Error::invalid(
                "references",
                "unresolved or inconsistent model reference",
            ));
        }
        warn(graph, options, kind, Some(&source), Some(&reference))?;
    }
    let mut branch_counts = BTreeMap::<&str, usize>::new();
    for edge in &graph.edges {
        if edge.kind == EdgeKind::Connection
            && edge
                .source_pin
                .as_ref()
                .is_some_and(|id| graph.pins[pins[id]].direction == PinDirection::Output)
        {
            *branch_counts.entry(&edge.source).or_default() += 1;
        }
    }
    for edge in &graph.edges {
        let Some(source_pin) = edge.source_pin.as_ref() else {
            continue;
        };
        if edge.kind != EdgeKind::Connection
            || graph.pins[pins[source_pin]].direction != PinDirection::Output
        {
            continue;
        }
        let target = nodes.get(&edge.target).map(|index| &graph.nodes[*index]);
        if branch_counts[edge.source.as_str()] > 1
            || target.is_some_and(|target| {
                target
                    .menu_text
                    .as_deref()
                    .is_some_and(|text| !text.is_empty())
            })
        {
            let text_source = if edge.label.as_deref().is_some_and(|text| !text.is_empty()) {
                ChoiceTextSource::EdgeLabel
            } else if target.is_some_and(|target| {
                target
                    .menu_text
                    .as_deref()
                    .is_some_and(|text| !text.is_empty())
            }) {
                ChoiceTextSource::TargetMenuText
            } else if target.is_some_and(|target| target.text.is_some()) {
                ChoiceTextSource::TargetText
            } else {
                ChoiceTextSource::None
            };
            graph.choices.push(Choice {
                edge: edge.id.clone(),
                source: edge.source.clone(),
                source_pin: source_pin.clone(),
                target: edge.target.clone(),
                text_source,
            });
        }
    }
    Ok(())
}

fn check_parent_cycles(nodes: &[Node], lookup: &BTreeMap<String, usize>) -> Result<()> {
    let mut done = BTreeSet::<&str>::new();
    for node in nodes {
        if done.contains(node.id.as_str()) {
            continue;
        }
        let mut visited = BTreeSet::<&str>::new();
        let mut current = Some(node.id.as_str());
        while let Some(id) = current {
            if done.contains(id) {
                break;
            }
            if !visited.insert(id) {
                return Err(Error::invalid("Parent", "cyclic model parent hierarchy"));
            }
            current = lookup
                .get(id)
                .and_then(|index| nodes[*index].parent.as_deref());
        }
        done.extend(visited);
    }
    Ok(())
}
