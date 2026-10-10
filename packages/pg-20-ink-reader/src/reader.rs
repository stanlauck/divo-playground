// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::*;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;

/// Read compiled runtime JSON (versions 19–21), returning a graph and findings.
/// Bounds are enforced before deserialization; unsupported content is retained.
pub fn read_compiled<R: Read>(
    reader: R,
    options: &ReadOptions,
) -> Result<(DialogueGraph, Report), Error> {
    if options.max_bytes == 0 || options.max_depth == 0 || options.max_elements == 0 {
        return Err(Error::Invalid("limits must be positive".into()));
    }
    // A hard stack safety ceiling also bounds user-supplied limits.
    if options.max_depth > 512 {
        return Err(Error::Invalid("max_depth must be at most 512".into()));
    }
    let mut bytes = Vec::new();
    reader
        .take(options.max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(Error::Io)?;
    if bytes.len() > options.max_bytes {
        return Err(Error::Limit("bytes"));
    }
    preflight(&bytes, options)?;
    let mut de = serde_json::Deserializer::from_slice(&bytes);
    de.disable_recursion_limit();
    let value = Value::deserialize(&mut de).map_err(Error::Json)?;
    de.end().map_err(Error::Json)?;
    count_elements(&value, options.max_elements)?;
    let top = value
        .as_object()
        .ok_or_else(|| Error::Invalid("top level must be an object".into()))?;
    let version = top
        .get("inkVersion")
        .and_then(Value::as_u64)
        .filter(|v| (19..=21).contains(v))
        .ok_or_else(|| Error::Invalid("inkVersion must be 19, 20 or 21".into()))?;
    let root = top
        .get("root")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("root must be a container array".into()))?;
    if top.get("listDefs").is_some_and(|v| !v.is_object()) {
        return Err(Error::Invalid("listDefs must be an object".into()));
    }
    let mut registry = BTreeMap::new();
    register(root, "", &mut registry)?;
    let mut b = Builder::new(registry, options, version);
    b.prepare(root);
    if let Some(decl) = b.registry.get("global decl").copied() {
        b.globals(decl);
    }
    if !body(root).is_empty()
        || top
            .get("listDefs")
            .is_some_and(|v| v.as_object().is_some_and(|m| !m.is_empty()))
    {
        let id = b.node("Root", None, NodeKind::FlowFragment, "Root");
        b.targets.insert("".into(), id.clone());
        let flow = b.content("", &id, &HashSet::new());
        b.enter(&id, flow.first.as_deref(), None);
        if let Some(lists) = top
            .get("listDefs")
            .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
        {
            let n = b.unsupported(&id, "", "lists", lists);
            for (exit, label) in flow.exits {
                b.edge(&exit, &n, EdgeKind::Connection, label);
            }
            if flow.first.is_none() {
                b.enter(&id, Some(&n), None);
            }
        }
    }
    let public = b.public.clone();
    for (path, canonical, parent_path, ty) in public {
        let id = b.reserved[&canonical].clone();
        let parent = parent_path.as_ref().map(|p| b.reserved[p].clone());
        b.node_reserved(&id, parent.as_deref(), NodeKind::FlowFragment, ty);
        b.set_name(&id, &canonical);
        if ty == "Knot" {
            b.report.stats.knots += 1;
        } else {
            b.report.stats.stitches += 1;
        }
        let flow = b.content(&path, &id, &HashSet::new());
        b.enter(&id, flow.first.as_deref(), None);
    }
    b.resolve_jumps();
    b.finish();
    Ok((b.graph, b.report))
}

fn preflight(bytes: &[u8], options: &ReadOptions) -> Result<(), Error> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for &c in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                quoted = false;
            }
        } else {
            match c {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > options.max_depth {
                        return Err(Error::Limit("depth"));
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}
fn count_elements(root: &Value, limit: usize) -> Result<(), Error> {
    let mut todo = vec![root];
    let mut count = 0;
    while let Some(v) = todo.pop() {
        count += 1;
        if count > limit {
            return Err(Error::Limit("elements"));
        }
        match v {
            Value::Array(a) => todo.extend(a),
            Value::Object(m) => todo.extend(m.values()),
            _ => {}
        }
    }
    Ok(())
}
fn trailer(a: &[Value]) -> Option<&Map<String, Value>> {
    a.last().and_then(Value::as_object).filter(|m| {
        m.is_empty()
            || m.keys().any(|k| k == "#f" || k == "#n")
            || (m.values().any(Value::is_array) && !m.contains_key("*") && !m.contains_key("flg"))
    })
}
fn body(a: &[Value]) -> &[Value] {
    if a.last().is_some_and(Value::is_null) || trailer(a).is_some() {
        &a[..a.len() - 1]
    } else {
        a
    }
}
fn child_path(path: &str, child: &str) -> String {
    if path.is_empty() {
        child.into()
    } else {
        format!("{path}.{child}")
    }
}
fn register<'a>(
    a: &'a [Value],
    path: &str,
    map: &mut BTreeMap<String, &'a [Value]>,
) -> Result<(), Error> {
    map.insert(path.into(), a);
    for (i, v) in body(a).iter().enumerate() {
        if let Some(sub) = v.as_array() {
            register(sub, &child_path(path, &i.to_string()), map)?;
        }
        if let Some(obj) = v.as_object() {
            for (key, sub) in obj {
                if key.starts_with('#') || key == "*" || key == "flg" {
                    continue;
                }
                if let Some(sub) = sub.as_array() {
                    register(sub, &child_path(path, key), map)?;
                }
            }
        }
    }
    if let Some(m) = trailer(a) {
        for (key, v) in m {
            if key.starts_with('#') {
                continue;
            }
            let sub = v
                .as_array()
                .ok_or_else(|| Error::Invalid(format!("named container {key} must be an array")))?;
            register(sub, &child_path(path, key), map)?;
        }
    }
    Ok(())
}
// An instruction object is a child of its container. The first ^ in .^.x
// removes that object step, hence it starts from the containing array itself.
fn resolve_path(current: &str, raw: &str) -> String {
    if !raw.starts_with('.') {
        return raw.into();
    }
    let mut parts: Vec<&str> = current.split('.').filter(|s| !s.is_empty()).collect();
    let mut steps = raw[1..].split('.').filter(|s| !s.is_empty());
    if let Some(first) = steps.next() {
        if first != "^" {
            parts.push(first);
        }
        for step in steps {
            if step == "^" {
                parts.pop();
            } else {
                parts.push(step);
            }
        }
    }
    parts.join(".")
}

fn path_candidates(current: &str, raw: &str) -> Vec<String> {
    let mut out = vec![resolve_path(current, raw)];
    if let Some(stripped) = raw.strip_prefix('.') {
        let mut base: Vec<&str> = current.split('.').filter(|s| !s.is_empty()).collect();
        let pieces: Vec<&str> = stripped.split('.').filter(|s| !s.is_empty()).collect();
        let ups = pieces.iter().take_while(|s| **s == "^").count();
        let suffix = pieces[ups..].join(".");
        while base.last().is_some_and(|s| s.parse::<usize>().is_ok()) {
            base.pop();
        }
        for _ in 0..ups.saturating_sub(1) {
            base.pop();
        }
        if !suffix.is_empty() {
            let mut candidate = base.join(".");
            if !candidate.is_empty() {
                candidate.push('.');
            }
            candidate.push_str(&suffix);
            out.push(candidate);
        }
        if let Some(last) = pieces.last() {
            let mut candidate = current.to_owned();
            if !candidate.is_empty() {
                candidate.push('.');
            }
            candidate.push_str(last);
            out.push(candidate);
        }
    }
    out
}
#[derive(Default)]
struct Flow {
    first: Option<String>,
    exits: Vec<(String, Option<String>)>,
}
struct Builder<'a> {
    registry: BTreeMap<String, &'a [Value]>,
    graph: DialogueGraph,
    report: Report,
    ids: HashSet<String>,
    reserved: BTreeMap<String, String>,
    public: Vec<(String, String, Option<String>, &'static str)>,
    targets: BTreeMap<String, String>,
    positions: HashMap<String, usize>,
    statements: HashMap<String, usize>,
    jumps: Vec<(String, String, String)>,
    active: HashSet<String>,
}
impl<'a> Builder<'a> {
    fn new(registry: BTreeMap<String, &'a [Value]>, o: &ReadOptions, version: u64) -> Self {
        Self { registry, graph: DialogueGraph {
            version: 1, text_mode: TextMode::Literal,
            metadata: json!({"source_format":"ink","generator":concat!(env!("CARGO_PKG_NAME")," ",env!("CARGO_PKG_VERSION")),"ink_version":version}).as_object().unwrap().clone(),
            definitions: vec![], packages: vec![Package { index:0,name:o.package_name.clone(),metadata:{ let mut m=Map::new(); m.insert("path".into(), Value::String(o.package_name.clone())); m },node_ids:vec![] }],
            nodes:vec![], pins:vec![], edges:vec![], choices:vec![], variable_namespaces:vec![VariableNamespace { name:"ink".into(),metadata:Map::new(),variables:vec![] }], variables:vec![], hierarchy:vec![], warnings:vec![],
        }, report: Report {version:1,ink_version:version,unsupported:vec![],warnings:vec![],stats:Stats::default()}, ids:HashSet::new(),reserved:BTreeMap::new(),public:vec![],targets:BTreeMap::new(),positions:HashMap::new(),statements:HashMap::new(),jumps:vec![],active:HashSet::new() }
    }
    fn prepare(&mut self, root: &[Value]) {
        let mut helpers = HashSet::new();
        for (path, a) in &self.registry {
            for v in body(a) {
                if let Some(target) = v.get("*").and_then(Value::as_str) {
                    helpers.insert(resolve_path(path, target));
                }
                if v.get("c").and_then(Value::as_bool) == Some(true) {
                    if let Some(target) = v.get("->").and_then(Value::as_str) {
                        helpers.insert(resolve_path(path, target));
                    }
                }
            }
        }
        if let Some(m) = trailer(root) {
            for (name, v) in m {
                if name.starts_with('#')
                    || name == "global decl"
                    || !v.is_array()
                    || helpers.contains(name)
                {
                    continue;
                }
                self.public.push((name.clone(), name.clone(), None, "Knot"));
                self.public_children(name, &helpers);
            }
        }
        for (path, canonical, _, _) in self.public.clone() {
            let id = self.issue(&canonical, &path);
            self.targets.insert(path.clone(), id.clone());
            self.targets.insert(canonical.clone(), id.clone());
            self.reserved.insert(canonical, id);
        }
    }
    fn public_children(&mut self, parent: &str, helpers: &HashSet<String>) {
        let knot = parent.split('.').next().unwrap_or(parent).to_owned();
        let prefix = format!("{parent}.");
        let paths: Vec<String> = self
            .registry
            .keys()
            .filter(|p| p.starts_with(&prefix))
            .cloned()
            .collect();
        for path in paths {
            let Some(name) = path.rsplit('.').next() else {
                continue;
            };
            if name.parse::<usize>().is_ok()
                || name.starts_with('#')
                || name == "s"
                || name == "b"
                || name == "c"
                || name.starts_with("c-")
                || name.starts_with("b-")
                || helpers.contains(&path)
                || path.matches('.').count() <= parent.matches('.').count()
            {
                continue;
            }
            let canonical = format!("{knot}.{name}");
            if self.public.iter().any(|(_, c, _, _)| c == &canonical) {
                continue;
            }
            self.public
                .push((path, canonical.clone(), Some(parent.into()), "Stitch"));
        }
    }
    fn issue(&mut self, preferred: &str, path: &str) -> String {
        let mut id = preferred.to_string();
        let mut suffix = 2;
        while self.ids.contains(&id)
            || self.ids.contains(&format!("{id}#in"))
            || self.ids.contains(&format!("{id}#out"))
        {
            id = format!("{preferred}~{suffix}");
            suffix += 1;
        }
        if id != preferred {
            self.warning(
                WarningKind::UnknownType,
                path,
                Some(preferred),
                "identifier collision was suffixed",
            );
        }
        self.ids.insert(id.clone());
        self.ids.insert(format!("{id}#in"));
        self.ids.insert(format!("{id}#out"));
        id
    }
    fn resolve_existing(&self, current: &str, raw: &str) -> String {
        path_candidates(current, raw)
            .into_iter()
            .find(|p| self.registry.contains_key(p) || self.targets.contains_key(p))
            .unwrap_or_else(|| resolve_path(current, raw))
    }
    fn node(&mut self, preferred: &str, parent: Option<&str>, kind: NodeKind, ty: &str) -> String {
        let id = self.issue(preferred, preferred);
        self.node_reserved(&id, parent, kind, ty);
        id
    }
    fn node_reserved(&mut self, id: &str, parent: Option<&str>, kind: NodeKind, ty: &str) {
        self.positions.insert(id.into(), self.graph.nodes.len());
        self.graph.packages[0].node_ids.push(id.into());
        self.graph.nodes.push(Node {
            id: id.into(),
            package: 0,
            source_type: ty.into(),
            kind,
            parent: parent.map(str::to_owned),
            technical_name: None,
            display_name: None,
            text: None,
            menu_text: None,
            speaker: None,
            script: None,
            input_pins: vec![format!("{id}#in")],
            output_pins: vec![format!("{id}#out")],
            properties: Map::new(),
            template: None,
            metadata: Map::new(),
        });
        for (direction, ending) in [(PinDirection::Input, "in"), (PinDirection::Output, "out")] {
            self.graph.pins.push(Pin {
                id: format!("{id}#{ending}"),
                owner: id.into(),
                direction,
                index: 0,
                script: None,
                properties: Map::new(),
            });
        }
    }
    fn leaf(&mut self, parent: &str, kind: NodeKind, ty: &str) -> String {
        let n = self.statements.entry(parent.into()).or_default();
        *n += 1;
        let preferred = format!("{parent}:{n}");
        self.node(&preferred, Some(parent), kind, ty)
    }
    fn get(&mut self, id: &str) -> &mut Node {
        &mut self.graph.nodes[self.positions[id]]
    }
    fn set_name(&mut self, id: &str, name: &str) {
        let n = self.get(id);
        n.technical_name = Some(name.into());
        n.display_name = Some(name.into());
    }
    fn warning(&mut self, kind: WarningKind, path: &str, reference: Option<&str>, message: &str) {
        self.graph.warnings.push(Warning {
            kind,
            source: Some(path.into()),
            reference: reference.map(str::to_owned),
        });
        self.report.warnings.push(Finding {
            kind: serde_json::to_value(kind).unwrap().as_str().unwrap().into(),
            path: path.into(),
            message: message.into(),
        });
    }
    fn unsupported(&mut self, parent: &str, path: &str, feature: &str, raw: &Value) -> String {
        let id = self.leaf(parent, NodeKind::Instruction, "Unsupported");
        self.get(&id).properties.insert("raw".into(), raw.clone());
        self.get(&id)
            .properties
            .insert("feature".into(), json!(feature));
        self.warning(
            WarningKind::UnknownType,
            path,
            None,
            &format!("unsupported feature: {feature}"),
        );
        if let Some(u) = self
            .report
            .unsupported
            .iter_mut()
            .find(|u| u.feature == feature)
        {
            u.count += 1;
            if u.paths.len() < 10 {
                u.paths.push(path.into());
            }
        } else {
            self.report.unsupported.push(UnsupportedFeature {
                feature: feature.into(),
                count: 1,
                paths: vec![path.into()],
            });
        }
        id
    }
    fn edge(
        &mut self,
        source: &str,
        target: &str,
        kind: EdgeKind,
        label: Option<String>,
    ) -> String {
        let id = format!("e{}", self.graph.edges.len());
        let index = self
            .graph
            .edges
            .iter()
            .filter(|e| e.source == source)
            .count();
        self.graph.edges.push(Edge {
            id: id.clone(),
            kind,
            source: source.into(),
            source_pin: Some(format!("{source}#out")),
            target: target.into(),
            target_pin: Some(format!("{target}#in")),
            index,
            label,
            properties: Map::new(),
        });
        id
    }
    fn enter(&mut self, source: &str, first: Option<&str>, label: Option<String>) {
        if let Some(first) = first {
            self.edge(source, first, EdgeKind::Connection, label);
        }
    }
    fn append(&mut self, flow: &mut Flow, next: Flow) {
        if let Some(first) = &next.first {
            for (exit, label) in &flow.exits {
                self.edge(exit, first, EdgeKind::Connection, label.clone());
            }
            if flow.first.is_none() {
                flow.first = Some(first.clone());
            }
            flow.exits = next.exits;
        }
    }
    fn single(id: String, terminal: bool) -> Flow {
        Flow {
            first: Some(id.clone()),
            exits: if terminal { vec![] } else { vec![(id, None)] },
        }
    }
    fn line(&mut self, parent: &str, text: String, tags: &mut Vec<String>) -> String {
        let id = self.leaf(parent, NodeKind::DialogueFragment, "Line");
        self.get(&id).text = Some(text);
        if !tags.is_empty() {
            self.get(&id)
                .properties
                .insert("tags".into(), json!(std::mem::take(tags)));
        }
        self.report.stats.lines += 1;
        id
    }
    fn flush(
        &mut self,
        parent: &str,
        text: &mut String,
        tags: &mut Vec<String>,
        flow: &mut Flow,
        last: &mut Option<String>,
    ) {
        if !text.is_empty() {
            let id = self.line(parent, std::mem::take(text), tags);
            *last = Some(id.clone());
            self.append(flow, Self::single(id, false));
        }
    }
    fn script(&mut self, id: &str, role: ScriptRole, text: String) {
        self.get(id).script = Some(Script {
            language: "ink".into(),
            role,
            text,
        });
    }
    fn assignment(&mut self, parent: &str, v: &Value, ops: &[Value]) -> String {
        let temp = v.get("temp=").is_some();
        let name = v
            .get(if temp { "temp=" } else { "VAR=" })
            .and_then(Value::as_str)
            .unwrap_or("?");
        let id = self.leaf(
            parent,
            NodeKind::Instruction,
            if temp { "Temp" } else { "Set" },
        );
        self.script(
            &id,
            ScriptRole::Instruction,
            format!("{name} = {}", expression(ops)),
        );
        self.get(&id).properties.insert(
            "reassignment".into(),
            json!(v.get("re").and_then(Value::as_bool).unwrap_or(false)),
        );
        id
    }
    fn content(&mut self, path: &str, parent: &str, _seen: &HashSet<String>) -> Flow {
        if self.active.contains(path) {
            let raw = json!({"path":path});
            let id = self.unsupported(parent, path, "recursive_container", &raw);
            return Self::single(id, false);
        }
        let Some(a) = self.registry.get(path).copied() else {
            return Flow::default();
        };
        self.active.insert(path.into());
        let mut values = body(a);
        // Ink emits a generated return trampoline before the real contents of
        // each choice target. It is runtime bookkeeping, so expose only the
        // authored statements after its newline delimiter.
        if path.rsplit('.').next().is_some_and(|s| s.starts_with("c-"))
            && !values
                .first()
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with('^'))
        {
            if let Some(n) = values.iter().position(|v| v.as_str() == Some("\n")) {
                values = &values[n + 1..];
            }
        }
        let flow = self.statements(path, parent, values);
        self.active.remove(path);
        flow
    }
    fn statements(&mut self, path: &str, parent: &str, values: &[Value]) -> Flow {
        let mut flow = Flow::default();
        let mut text = String::new();
        let mut tags = Vec::new();
        let mut last = None;
        let mut eval = Vec::new();
        let mut menu_parts = Vec::new();
        let mut i = 0;
        while i < values.len() {
            let v = &values[i];
            // Choice compiler wrappers are siblings, each with its own text evaluation.
            if v.get("*").is_some()
                || v.as_array()
                    .is_some_and(|a| body(a).iter().any(|v| v.get("*").is_some()))
            {
                self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                let mut specs = Vec::new();
                while i < values.len() {
                    let v = &values[i];
                    if v.get("*").is_some() {
                        specs.push((path.to_string(), v.clone(), menu_parts.join("")));
                        menu_parts.clear();
                    } else if let Some(a) = v
                        .as_array()
                        .filter(|a| body(a).iter().any(|v| v.get("*").is_some()))
                    {
                        let wrapper = child_path(path, &i.to_string());
                        let mut parts = Vec::new();
                        for item in body(a) {
                            if let Some(raw) = item.get("*") {
                                let mut menu = parts.join("");
                                if menu.is_empty() {
                                    menu = self
                                        .static_text(&format!("{wrapper}.s"), &mut HashSet::new())
                                        .unwrap_or_default();
                                }
                                specs.push((wrapper.clone(), item.clone(), menu));
                                parts.clear();
                                let _ = raw;
                            } else if let Some(st) = item.as_str().and_then(|s| s.strip_prefix('^'))
                            {
                                parts.push(st.to_string());
                            } else if let Some(target) = item.get("f()").and_then(Value::as_str) {
                                // The documented reusable choice start-content helper is static text.
                                let resolved = self.resolve_existing(&wrapper, target);
                                if let Some(t) = self.static_text(&resolved, &mut HashSet::new()) {
                                    parts.push(t);
                                } else {
                                    let id = self.unsupported(parent, &wrapper, "functions", item);
                                    self.append(&mut flow, Self::single(id, false));
                                }
                            }
                        }
                    } else {
                        break;
                    }
                    i += 1;
                }
                let hub = self.leaf(parent, NodeKind::Hub, "ChoicePoint");
                let index = self.statements.get(parent).copied().unwrap_or(1) - 1;
                self.get(&hub)
                    .properties
                    .insert("index".into(), json!(index));
                let mut exits = Vec::new();
                for (current, raw, menu) in specs {
                    let raw_target = raw.get("*").and_then(Value::as_str).unwrap_or("?");
                    let target = self.resolve_existing(&current, raw_target);
                    let output = self.static_text(&target, &mut HashSet::new());
                    let menu = if menu.is_empty() {
                        output.clone().unwrap_or_else(|| raw_target.into())
                    } else {
                        menu
                    };
                    let option = self.leaf(&hub, NodeKind::DialogueFragment, "Option");
                    self.get(&option).menu_text = Some(menu.clone());
                    self.get(&option).text = output.filter(|t| t != &menu);
                    self.get(&option).properties.insert(
                        "flags".into(),
                        json!(flags(raw.get("flg").and_then(Value::as_u64).unwrap_or(0))),
                    );
                    self.get(&option)
                        .properties
                        .insert("target".into(), json!(raw_target));
                    let eid = self.edge(&hub, &option, EdgeKind::Connection, None);
                    self.graph.choices.push(Choice {
                        edge: eid,
                        source: hub.clone(),
                        source_pin: format!("{hub}#out"),
                        target: option.clone(),
                        text_source: ChoiceTextSource::TargetMenuText,
                    });
                    self.report.stats.choices += 1;
                    if self.registry.contains_key(&target) {
                        let cf = self.content(&target, &option, &HashSet::new());
                        self.enter(&option, cf.first.as_deref(), None);
                        if cf.first.is_none() {
                            exits.push((option, None));
                        } else {
                            exits.extend(cf.exits);
                        }
                    } else {
                        self.warning(
                            WarningKind::MissingNode,
                            &current,
                            Some(raw_target),
                            "choice target is missing; literal path retained as menu text",
                        );
                        exits.push((option, None));
                    }
                }
                self.append(
                    &mut flow,
                    Flow {
                        first: Some(hub),
                        exits,
                    },
                );
                continue;
            }
            if v.as_str() == Some("ev") {
                let end = values[i + 1..]
                    .iter()
                    .position(|v| v.as_str() == Some("/ev"))
                    .map(|n| i + 1 + n);
                if let Some(end) = end {
                    let ops = &values[i + 1..end];
                    // Build strings used on the evaluation stack for menu labels.
                    let mut j = 0;
                    let mut expression_ops = Vec::new();
                    while j < ops.len() {
                        if ops[j].as_str() == Some("str") {
                            let stop = ops[j + 1..]
                                .iter()
                                .position(|v| v.as_str() == Some("/str"))
                                .map(|n| j + 1 + n)
                                .unwrap_or(ops.len());
                            let mut part = String::new();
                            for item in &ops[j + 1..stop] {
                                if let Some(t) = item.as_str().and_then(|s| s.strip_prefix('^')) {
                                    part.push_str(t);
                                } else if let Some(target) = item.get("f()").and_then(Value::as_str)
                                {
                                    if let Some(t) = self.static_text(
                                        &self.resolve_existing(path, target),
                                        &mut HashSet::new(),
                                    ) {
                                        part.push_str(&t)
                                    } else {
                                        let id = self.unsupported(parent, path, "functions", item);
                                        self.append(&mut flow, Self::single(id, false));
                                    }
                                } else if item.as_str() != Some("<>") {
                                    expression_ops.push(item.clone());
                                }
                            }
                            menu_parts.push(part.clone());
                            expression_ops.push(json!(format!("^{part}")));
                            j = stop.saturating_add(1);
                            continue;
                        }
                        if ops[j].as_str() == Some("out") {
                            text.push('{');
                            text.push_str(&expression(&expression_ops));
                            text.push('}');
                            expression_ops.clear();
                        } else if ops[j].get("VAR=").is_some() || ops[j].get("temp=").is_some() {
                            self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                            let id = self.assignment(parent, &ops[j], &expression_ops);
                            self.append(&mut flow, Self::single(id, false));
                            expression_ops.clear();
                        } else if let Some(feature) = unsupported_feature(&ops[j]) {
                            self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                            let id = self.unsupported(parent, path, feature, &ops[j]);
                            self.append(&mut flow, Self::single(id, false));
                            expression_ops.push(ops[j].clone());
                        } else {
                            expression_ops.push(ops[j].clone());
                        }
                        j += 1;
                    }
                    eval = expression_ops;
                    i = end + 1;
                    continue;
                }
                self.warning(
                    WarningKind::MissingScript,
                    path,
                    None,
                    "unterminated evaluation block",
                );
            }
            if v.get("c").and_then(Value::as_bool) == Some(true)
                && v.get("->").is_some()
                && v.get("var").and_then(Value::as_bool) != Some(true)
            {
                self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                let id = self.leaf(parent, NodeKind::Condition, "Condition");
                self.script(&id, ScriptRole::Condition, expression(&eval));
                eval.clear();
                let raw = v.get("->").and_then(Value::as_str).unwrap_or("?");
                let target = self.resolve_existing(path, raw);
                let mut branches = vec![(target.clone(), "true")];
                // Ink's inline true branch is named `b`; the following
                // anonymous container is its false branch.
                if values
                    .get(i + 1)
                    .and_then(Value::as_object)
                    .and_then(|o| o.get("b"))
                    .is_some_and(Value::is_array)
                {
                    branches[0].0 = child_path(path, "b");
                    i += 1;
                    if values.get(i + 1).is_some_and(Value::is_array) {
                        branches.push((child_path(path, &(i + 1).to_string()), "false"));
                        i += 1;
                    }
                // Ink's inline else branch is the following anonymous container.
                } else if values.get(i + 1).is_some_and(Value::is_array) {
                    branches.push((child_path(path, &(i + 1).to_string()), "false"));
                    i += 1;
                // Common compiled two-way form: conditional branch divert followed by an unconditional else divert.
                } else if let Some(next) = values.get(i + 1).filter(|v| {
                    v.get("->").is_some() && v.get("c").and_then(Value::as_bool) != Some(true)
                }) {
                    let other = self.resolve_existing(
                        path,
                        next.get("->").and_then(Value::as_str).unwrap_or("?"),
                    );
                    if self.registry.contains_key(&other) && other != target {
                        branches.push((other, "false"));
                        i += 1;
                    }
                } else {
                    let prefix = target.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
                    let other = child_path(prefix, "b-1");
                    if target.ends_with("b-0") && self.registry.contains_key(&other) {
                        branches.push((other, "false"));
                    }
                }
                let mut exits = Vec::new();
                for (branch, label) in branches {
                    if self.registry.contains_key(&branch) {
                        let begin = self.graph.nodes.len();
                        let cf = self.content(&branch, &id, &HashSet::new());
                        for n in &mut self.graph.nodes[begin..] {
                            if n.parent.as_deref() == Some(&id) {
                                n.properties.insert("clause".into(), json!(label));
                            }
                        }
                        self.enter(&id, cf.first.as_deref(), Some(label.into()));
                        if cf.first.is_none() {
                            exits.push((id.clone(), Some(label.into())))
                        } else {
                            exits.extend(cf.exits);
                        }
                    } else {
                        self.warning(
                            WarningKind::MissingNode,
                            path,
                            Some(raw),
                            "conditional target is missing",
                        );
                        exits.push((id.clone(), Some(label.into())));
                    }
                }
                if exits.is_empty() { /* both branches terminate */
                } else if !self
                    .graph
                    .edges
                    .iter()
                    .any(|e| e.source == id && e.label.as_deref() == Some("false"))
                {
                    exits.push((id.clone(), Some("false".into())));
                }
                self.append(
                    &mut flow,
                    Flow {
                        first: Some(id),
                        exits,
                    },
                );
                i += 1;
                continue;
            }
            if let Some(feature) = unsupported_feature(v) {
                // Choice start-content helpers are the sole statically expanded function-call pattern.
                if feature == "functions" && parent.contains(':') {
                    if let Some(target) = v.get("f()").and_then(Value::as_str) {
                        if let Some(t) = self
                            .static_text(&self.resolve_existing(path, target), &mut HashSet::new())
                        {
                            text.push_str(&t);
                            i += 1;
                            continue;
                        }
                    }
                }
                self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                let id = self.unsupported(parent, path, feature, v);
                self.append(&mut flow, Self::single(id, false));
                i += 1;
                continue;
            }
            if let Some(s) = v.as_str() {
                match s {
                    "\n" => self.flush(parent, &mut text, &mut tags, &mut flow, &mut last),
                    "<>" => {
                        if text.is_empty() {
                            let id = self.unsupported(parent, path, "glue_across_elements", v);
                            self.append(&mut flow, Self::single(id, false));
                        }
                    }
                    "#" => {
                        let stop = values[i + 1..]
                            .iter()
                            .position(|v| v.as_str() == Some("/#"))
                            .map(|n| i + 1 + n)
                            .unwrap_or(values.len());
                        let tag = values[i + 1..stop]
                            .iter()
                            .filter_map(|v| v.as_str().and_then(|s| s.strip_prefix('^')))
                            .collect::<String>();
                        tags.push(tag);
                        i = stop;
                    }
                    "done" | "end" => {
                        self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                        let id = self.leaf(parent, NodeKind::Jump, "Divert");
                        self.get(&id)
                            .properties
                            .insert("target".into(), json!(s.to_uppercase()));
                        self.report.stats.diverts += 1;
                        self.append(&mut flow, Self::single(id, true));
                    }
                    "out" => {
                        text.push('{');
                        text.push_str(&expression(&eval));
                        text.push('}');
                        eval.clear();
                    }
                    "nop" | "pop" | "du" | "/ev" | "str" | "/str" | "/#" => {}
                    _ => {
                        if let Some(t) = s.strip_prefix('^') {
                            text.push_str(t);
                        } else {
                            eval.push(v.clone());
                        }
                    }
                }
            } else if let Some(o) = v.as_object() {
                if let Some(tag) = o.get("#").and_then(Value::as_str) {
                    tags.push(tag.into());
                } else if let Some(target) = o.get("->").and_then(Value::as_str) {
                    if target.starts_with('.')
                        && target
                            .rsplit('.')
                            .next()
                            .is_some_and(|s| s.parse::<usize>().is_ok())
                    {
                        // Compiler-generated branch rejoin pointer.
                        i += 1;
                        continue;
                    }
                    self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                    if o.get("var").and_then(Value::as_bool) == Some(true) {
                        let id = self.unsupported(parent, path, "variable_diverts", v);
                        self.append(&mut flow, Self::single(id, false));
                        i += 1;
                        continue;
                    }
                    let id = self.leaf(parent, NodeKind::Jump, "Divert");
                    self.get(&id)
                        .properties
                        .insert("target".into(), json!(target));
                    if !matches!(target, "DONE" | "END") {
                        self.jumps.push((id.clone(), path.into(), target.into()));
                    }
                    self.report.stats.diverts += 1;
                    self.append(&mut flow, Self::single(id, true));
                } else if o.contains_key("VAR=") || o.contains_key("temp=") {
                    self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                    let id = self.assignment(parent, v, &eval);
                    eval.clear();
                    self.append(&mut flow, Self::single(id, false));
                } else if o.contains_key("VAR?") {
                    eval.push(v.clone());
                } else {
                    self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                    let id = self.unsupported(parent, path, "unknown_object", v);
                    self.append(&mut flow, Self::single(id, false));
                }
            } else if let Some(_a) = v.as_array() {
                self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
                let nested = child_path(path, &i.to_string());
                let mut cf = self.content(&nested, parent, &HashSet::new());
                // Inline conditionals keep their false container as the next
                // sibling of the condition container in the parent flow.
                if let Some(condition) = cf
                    .first
                    .clone()
                    .filter(|id| self.get(id).source_type == "Condition")
                {
                    if values.get(i + 1).is_some_and(Value::is_array) {
                        let false_path = child_path(path, &(i + 1).to_string());
                        let begin = self.graph.nodes.len();
                        let ff = self.content(&false_path, &condition, &HashSet::new());
                        for n in &mut self.graph.nodes[begin..] {
                            if n.parent.as_deref() == Some(&condition) {
                                n.properties.insert("clause".into(), json!("false"));
                            }
                        }
                        self.enter(&condition, ff.first.as_deref(), Some("false".into()));
                        cf.exits.retain(|(exit, _)| exit != &condition);
                        cf.exits.extend(ff.exits);
                        i += 1;
                    }
                }
                self.append(&mut flow, cf);
            } else if v.is_number() || v.is_boolean() {
                eval.push(v.clone());
            }
            i += 1;
        }
        self.flush(parent, &mut text, &mut tags, &mut flow, &mut last);
        if !tags.is_empty() {
            if let Some(id) = last {
                let existing = self.get(&id).properties.entry("tags").or_insert(json!([]));
                if let Some(a) = existing.as_array_mut() {
                    a.extend(tags.into_iter().map(Value::String));
                }
            }
        }
        flow
    }
    fn static_text(&self, target: &str, seen: &mut HashSet<String>) -> Option<String> {
        if !seen.insert(target.into()) {
            return None;
        }
        let a = self.registry.get(target)?;
        let mut text = String::new();
        let mut values = body(a);
        if target
            .rsplit('.')
            .next()
            .is_some_and(|s| s.starts_with("c-"))
            && !values
                .first()
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with('^'))
        {
            if let Some(n) = values.iter().position(|v| v.as_str() == Some("\n")) {
                values = &values[n + 1..];
            }
        }
        for v in values {
            if let Some(t) = v.as_str().and_then(|s| s.strip_prefix('^')) {
                text.push_str(t);
            } else if v.as_str() == Some("\n") {
                break;
            } else if let Some(raw) = v.get("f()").and_then(Value::as_str) {
                text.push_str(&self.static_text(&self.resolve_existing(target, raw), seen)?);
            } else if !v
                .as_str()
                .is_some_and(|s| matches!(s, "<>" | "nop" | "str" | "/str" | "ev" | "/ev"))
            {
                break;
            }
        }
        seen.remove(target);
        (!text.is_empty()).then_some(text)
    }
    fn variable(&mut self, name: Option<&Value>, global: bool, ops: &[Value]) {
        let Some(name) = name.and_then(Value::as_str) else {
            return;
        };
        if !global {
            return;
        }
        let value = ops.last().cloned().unwrap_or(Value::Null);
        let kind = match value {
            Value::Bool(_) => VariableKind::Boolean,
            Value::Number(ref n) => {
                if n.is_i64() {
                    VariableKind::Integer
                } else {
                    VariableKind::Float
                }
            }
            Value::String(_) => VariableKind::String,
            _ => VariableKind::Other,
        };
        self.graph.variable_namespaces[0]
            .variables
            .push(name.into());
        self.graph.variables.push(Variable {
            namespace: "ink".into(),
            name: name.into(),
            source_type: "VAR".into(),
            kind,
            value: value.clone(),
            raw_value: value,
            description: None,
            metadata: Map::new(),
        });
        self.report.stats.variables = self.graph.variables.len();
    }
    fn globals(&mut self, idx: &[Value]) {
        let mut eval = Vec::new();
        for value in body(idx) {
            if let Some(object) = value.as_object() {
                if object.contains_key("VAR=") {
                    let ops = eval.clone();
                    self.variable(object.get("VAR="), true, &ops);
                    eval.clear();
                } else if object.contains_key("temp=") {
                    eval.clear();
                } else {
                    eval.push(value.clone());
                }
            } else if !value.as_str().is_some_and(|s| matches!(s, "ev" | "/ev")) {
                eval.push(value.clone());
            }
        }
    }
    fn resolve_jumps(&mut self) {
        for (id, current, target) in std::mem::take(&mut self.jumps) {
            let resolved = self.resolve_existing(&current, &target);
            if let Some(t) = self
                .targets
                .get(&resolved)
                .or_else(|| self.targets.get(&target))
                .cloned()
            {
                self.edge(&id, &t, EdgeKind::Jump, None);
            } else {
                self.warning(
                    WarningKind::MissingNode,
                    &current,
                    Some(&target),
                    "divert target is missing",
                );
            }
        }
    }
    fn finish(&mut self) {
        let nodes = self.graph.nodes.clone();
        let mut depths = HashMap::new();
        let mut sibling_indexes: HashMap<Option<String>, usize> = HashMap::new();
        for n in nodes {
            let depth = hierarchy_depth(&n.id, &self.graph.nodes, &mut depths, 0);
            let key = n.parent.clone();
            let index = sibling_indexes.entry(key).or_default();
            let sibling_index = *index;
            *index += 1;
            self.graph.hierarchy.push(HierarchyEntry {
                id: n.id.clone(),
                parent: n.parent.clone(),
                index: sibling_index,
                depth,
                properties: Map::new(),
            });
        }
        self.report
            .unsupported
            .sort_by(|a, b| a.feature.cmp(&b.feature));
    }
}
fn flags(n: u64) -> Vec<&'static str> {
    let mut v = Vec::new();
    for (bit, name) in [
        (1, "has_condition"),
        (2, "has_start_content"),
        (4, "has_choice_only_content"),
        (8, "invisible_default"),
        (16, "once_only"),
    ] {
        if n & bit != 0 {
            v.push(name)
        }
    }
    v
}
fn unsupported_feature(value: &Value) -> Option<&'static str> {
    let object = value.as_object();
    if let Some(s) = value.as_str() {
        return match s {
            "thread" => Some("threads"),
            "seq" | "shuffle" => Some("sequences"),
            "->->" | "~ret" => Some("returns"),
            _ => None,
        };
    }
    let object = object?;
    if object.contains_key("->t->") {
        Some("tunnels")
    } else if object.contains_key("f()") {
        Some("functions")
    } else if object.contains_key("x()") {
        Some("external_calls")
    } else if object.contains_key("^->") {
        Some("variable_diverts")
    } else if object.contains_key("^var") {
        Some("variable_pointers")
    } else if object.contains_key("CNT?") {
        Some("read_counts")
    } else if object.contains_key("list") {
        Some("lists")
    } else {
        None
    }
}
fn hierarchy_depth(
    id: &str,
    nodes: &[Node],
    memo: &mut HashMap<String, usize>,
    guard: usize,
) -> usize {
    if guard > nodes.len() {
        return 0;
    }
    if let Some(depth) = memo.get(id) {
        return *depth;
    }
    let depth = nodes
        .iter()
        .find(|node| node.id == id)
        .and_then(|node| node.parent.as_deref())
        .map(|parent| hierarchy_depth(parent, nodes, memo, guard + 1) + 1)
        .unwrap_or(0);
    memo.insert(id.to_owned(), depth);
    depth
}
fn expression(ops: &[Value]) -> String {
    let mut stack: Vec<String> = Vec::new();
    let infix = [
        "+", "-", "*", "/", "%", "==", "!=", ">", "<", ">=", "<=", "&&", "||",
    ];
    for v in ops {
        if let Value::String(op) = v {
            if infix.contains(&op.as_str()) && stack.len() >= 2 {
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();
                stack.push(format!("{left} {op} {right}"));
                continue;
            }
            if op == "!" && !stack.is_empty() {
                let value = stack.pop().unwrap();
                stack.push(format!("!{value}"));
                continue;
            }
        }
        stack.push(expression_atom(v));
    }
    if !stack.is_empty() {
        return stack.join(" ");
    }
    String::new()
}

fn expression_atom(v: &Value) -> String {
    match v {
        Value::String(s) if s.starts_with('^') => s[1..].to_owned(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Object(o) => o
            .get("VAR?")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| serde_json::to_string(v).unwrap()),
        _ => v.to_string(),
    }
}
