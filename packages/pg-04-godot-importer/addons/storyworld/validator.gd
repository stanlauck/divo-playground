# SPDX-License-Identifier: MIT OR Apache-2.0
# gdlint: disable=max-returns
# Fail-fast checks retain the first sanitized error without deep nesting.
@tool
extends RefCounted

const MAX_RECORDS := 50_000
const MAX_ID_BYTES := 128
const MAX_DSL_DEPTH := 32
const MAX_WARNINGS := 50_000
const NODE_KINDS := [
	"flow_fragment",
	"dialogue",
	"dialogue_fragment",
	"hub",
	"jump",
	"condition",
	"instruction",
	"entity",
	"user_folder",
	"comment",
	"asset",
	"other"
]
const WARNING_KINDS := [
	"missing_node",
	"missing_pin",
	"missing_parent",
	"missing_speaker",
	"missing_hierarchy_object",
	"hierarchy_parent_mismatch",
	"unknown_type",
	"unknown_variable_type",
	"missing_script",
	"localization_keys"
]

var _error: Dictionary = {}
var _warnings: Array[Dictionary] = []
var _nodes: Dictionary = {}
var _pins: Dictionary = {}
var _edges: Dictionary = {}
var _variables: Dictionary = {}
var _locations: Dictionary = {}
var _world_ids: Dictionary = {}


func validate(document: Variant) -> Dictionary:
	_error = {}
	_warnings = []
	_nodes = {}
	_pins = {}
	_edges = {}
	_variables = {}
	_locations = {}
	_world_ids = {}
	if not _object(document, ["version", "world", "dialogue"], [], "$"):
		return _result()
	if typeof(document.version) != TYPE_INT or document.version != 1:
		_fail("unsupported_version", "$.version")
		return _result()
	if _dialogue(document.dialogue):
		_world(document.world)
	if _error.is_empty():
		for node: Dictionary in document.dialogue.nodes:
			_declarative(node, "$.dialogue.nodes")
		for choice: Dictionary in document.dialogue.choices:
			_declarative(choice, "$.dialogue.choices")
	return _result()


func _result() -> Dictionary:
	if not _error.is_empty():
		return {"ok": false, "error": _error}
	return {"ok": true, "warnings": _warnings}


func _fail(code: String, path: String) -> bool:
	if _error.is_empty():
		_error = {"code": code, "path": path}
	return false


func _object(value: Variant, required: Array, optional: Array, path: String) -> bool:
	if typeof(value) != TYPE_DICTIONARY:
		return _fail("expected_object", path)
	for key: Variant in required:
		if not value.has(key):
			return _fail("missing_field", path + "." + str(key))
	for key: Variant in value:
		if not key in required and not key in optional:
			return _fail("unknown_field", path)
	return true


func _dictionary(value: Variant, path: String) -> bool:
	return true if typeof(value) == TYPE_DICTIONARY else _fail("expected_object", path)


func _array(value: Variant, path: String) -> bool:
	if typeof(value) != TYPE_ARRAY:
		return _fail("expected_array", path)
	if value.size() > MAX_RECORDS:
		return _fail("record_limit", path)
	return true


func _string(value: Variant, path: String, nullable := false) -> bool:
	if nullable and value == null:
		return true
	return true if typeof(value) == TYPE_STRING else _fail("expected_string", path)


func _integer(value: Variant, path: String) -> bool:
	return true if typeof(value) == TYPE_INT and value >= 0 else _fail("expected_index", path)


func _id(value: Variant, path: String, nullable := false) -> bool:
	if nullable and value == null:
		return true
	if not _string(value, path):
		return false
	if (
		value.is_empty()
		or value.strip_edges() != value
		or value.to_utf8_buffer().size() > MAX_ID_BYTES
	):
		return _fail("invalid_id", path)
	for i in range(value.length()):
		var point: int = value.unicode_at(i)
		if point < 0x20 or (point >= 0x7f and point <= 0x9f):
			return _fail("invalid_id", path)
	return true


func _nonzero_id(value: Variant, path: String) -> bool:
	if not _id(value, path):
		return false
	if value.begins_with("0x") or value.begins_with("0X"):
		var suffix: String = value.substr(2)
		if not suffix.is_empty() and suffix.replace("0", "").is_empty():
			return _fail("zero_id", path)
	return true


func _reference_id(value: Variant, path: String, nullable := false) -> bool:
	return true if nullable and value == null else _nonzero_id(value, path)


func _ids(value: Variant, path: String, nonzero := true) -> bool:
	if not _array(value, path):
		return false
	var seen: Dictionary = {}
	for id: Variant in value:
		if not (_nonzero_id(id, path) if nonzero else _id(id, path)):
			return false
		if seen.has(id):
			return _fail("duplicate_id", path)
		seen[id] = true
	return true


func _enum(value: Variant, allowed: Array, path: String) -> bool:
	return true if value in allowed else _fail("invalid_enum", path)


func _warning(
	code: String, path: String, source: Variant = null, reference: Variant = null
) -> void:
	if _warnings.size() >= MAX_WARNINGS:
		_fail("warning_limit", path)
	else:
		_warnings.append({"code": code, "path": path, "source": source, "reference": reference})


func _script(value: Variant, path: String) -> bool:
	if value == null:
		return true
	if not _object(value, ["language", "role", "text"], [], path):
		return false
	if not _string(value.language, path) or not _string(value.text, path):
		return false
	if not _enum(value.role, ["condition", "instruction"], path):
		return false
	_warning("opaque_script_not_executed", path)
	return true


func _dialogue(value: Variant) -> bool:
	var path := "$.dialogue"
	var fields := [
		"version",
		"text_mode",
		"metadata",
		"definitions",
		"packages",
		"nodes",
		"pins",
		"edges",
		"choices",
		"variable_namespaces",
		"variables",
		"hierarchy",
		"warnings"
	]
	if not _object(value, fields, [], path):
		return false
	if typeof(value.version) != TYPE_INT or value.version != 1:
		return _fail("unsupported_version", path + ".version")
	if not _enum(value.text_mode, ["literal", "localization_keys"], path):
		return false
	if not _dictionary(value.metadata, path + ".metadata"):
		return false
	for field: String in fields.slice(3):
		if not _array(value[field], path + "." + field):
			return false
	if not _dialogue_records(value):
		return false
	if not _dialogue_variables(value):
		return false
	return _dialogue_references(value)


func _dialogue_records(graph: Dictionary) -> bool:
	for i in range(graph.nodes.size()):
		var path := "$.dialogue.nodes[%d]" % i
		var node: Variant = graph.nodes[i]
		var fields := [
			"id",
			"package",
			"source_type",
			"kind",
			"parent",
			"technical_name",
			"display_name",
			"text",
			"menu_text",
			"speaker",
			"script",
			"input_pins",
			"output_pins",
			"properties",
			"template",
			"metadata"
		]
		if not _object(node, fields, ["condition", "event"], path):
			return false
		if not _nonzero_id(node.id, path) or _nodes.has(node.id):
			return _fail("duplicate_id", path)
		_nodes[node.id] = node
		if not _integer(node.package, path) or node.package >= graph.packages.size():
			return _fail("invalid_package", path)
		if not _id(node.source_type, path) or not _enum(node.kind, NODE_KINDS, path):
			return false
		for field: String in ["parent", "speaker"]:
			if not _reference_id(node[field], path + "." + field, true):
				return false
		for field: String in ["technical_name", "display_name", "text", "menu_text"]:
			if not _string(node[field], path + "." + field, true):
				return false
		if not _script(node.script, path + ".script"):
			return false
		if not _ids(node.input_pins, path) or not _ids(node.output_pins, path):
			return false
		if not _dictionary(node.properties, path) or not _dictionary(node.metadata, path):
			return false
	for i in range(graph.pins.size()):
		var path := "$.dialogue.pins[%d]" % i
		var pin: Variant = graph.pins[i]
		if not _object(
			pin, ["id", "owner", "direction", "index", "script", "properties"], [], path
		):
			return false
		if not _nonzero_id(pin.id, path) or _nodes.has(pin.id) or _pins.has(pin.id):
			return _fail("duplicate_id", path)
		_pins[pin.id] = pin
		if not _reference_id(pin.owner, path) or not _integer(pin.index, path):
			return false
		if not _enum(pin.direction, ["input", "output"], path):
			return false
		if not _script(pin.script, path + ".script") or not _dictionary(pin.properties, path):
			return false
	for i in range(graph.edges.size()):
		var path := "$.dialogue.edges[%d]" % i
		var edge: Variant = graph.edges[i]
		if not _object(
			edge,
			[
				"id",
				"kind",
				"source",
				"source_pin",
				"target",
				"target_pin",
				"index",
				"label",
				"properties"
			],
			[],
			path
		):
			return false
		if not _nonzero_id(edge.id, path) or _edges.has(edge.id):
			return _fail("duplicate_edge", path)
		_edges[edge.id] = edge
		if not _enum(edge.kind, ["connection", "jump"], path) or not _integer(edge.index, path):
			return false
		for field: String in ["source", "target"]:
			if not _reference_id(edge[field], path + "." + field):
				return false
		for field: String in ["source_pin", "target_pin"]:
			if not _reference_id(edge[field], path + "." + field, true):
				return false
		if not _string(edge.label, path, true) or not _dictionary(edge.properties, path):
			return false
	return true


func _dialogue_variables(graph: Dictionary) -> bool:
	var namespaces: Dictionary = {}
	var namespace_members: Dictionary = {}
	for i in range(graph.variable_namespaces.size()):
		var path := "$.dialogue.variable_namespaces[%d]" % i
		var record: Variant = graph.variable_namespaces[i]
		if not _object(record, ["name", "metadata", "variables"], [], path):
			return false
		if not _id(record.name, path) or namespaces.has(record.name):
			return _fail("duplicate_namespace", path)
		if not _dictionary(record.metadata, path) or not _ids(record.variables, path, false):
			return false
		namespaces[record.name] = record
		namespace_members[record.name] = {}
		for name: String in record.variables:
			namespace_members[record.name][name] = true
		_variables[record.name] = {}
	for i in range(graph.variables.size()):
		var path := "$.dialogue.variables[%d]" % i
		var variable: Variant = graph.variables[i]
		if not _object(
			variable,
			[
				"namespace",
				"name",
				"source_type",
				"kind",
				"value",
				"raw_value",
				"description",
				"metadata"
			],
			[],
			path
		):
			return false
		if (
			not _id(variable.namespace, path)
			or not _id(variable.name, path)
			or not _id(variable.source_type, path)
		):
			return false
		if not namespaces.has(variable.namespace):
			return _fail("missing_namespace", path)
		if _variables[variable.namespace].has(variable.name):
			return _fail("duplicate_variable", path)
		if not namespace_members[variable.namespace].has(variable.name):
			return _fail("namespace_membership", path)
		if not _enum(variable.kind, ["boolean", "integer", "float", "string", "other"], path):
			return false
		if not _typed_literal(variable.value, variable.kind, path, true):
			return false
		if (
			not _string(variable.description, path, true)
			or not _dictionary(variable.metadata, path)
		):
			return false
		_variables[variable.namespace][variable.name] = variable
	for name: String in namespaces:
		if namespaces[name].variables.size() != _variables[name].size():
			return _fail("namespace_membership", "$.dialogue.variable_namespaces")
	return true


func _typed_literal(value: Variant, kind: String, path: String, allow_other := false) -> bool:
	match kind:
		"boolean":
			if typeof(value) == TYPE_BOOL:
				return true
		"integer":
			if typeof(value) == TYPE_INT:
				return true
		"float":
			if (
				(typeof(value) == TYPE_INT or typeof(value) == TYPE_FLOAT)
				and is_finite(float(value))
			):
				if (
					typeof(value) == TYPE_INT
					and (value > 9007199254740991 or value < -9007199254740991)
				):
					return _fail("lossy_float_literal", path)
				return true
		"string":
			if typeof(value) == TYPE_STRING:
				return true
		"other":
			if allow_other:
				return true
	return _fail("variable_type_mismatch", path)


func _node_reference(id: Variant, path: String, source: Variant, allow_missing := true) -> bool:
	if id == null:
		return true
	if _pins.has(id):
		return _fail("node_reference_is_pin", path)
	if not _nodes.has(id):
		if not allow_missing:
			return _fail("missing_dialogue_node", path)
		_warning("missing_dialogue_node", path, source, id)
	return true


func _pin_reference(id: Variant, owner: String, path: String) -> bool:
	# PG-03 retains source records on either pin side, including container inputs.
	# Ownership is required; imposing output-to-input polarity would change its graph.
	if id == null:
		return true
	if _nodes.has(id):
		return _fail("pin_reference_is_node", path)
	if not _pins.has(id):
		_warning("missing_dialogue_pin", path, owner, id)
	elif _pins[id].owner != owner:
		return _fail("pin_owner_mismatch", path)
	return true


func _dialogue_references(graph: Dictionary) -> bool:
	var package_members: Dictionary = {}
	for i in range(graph.packages.size()):
		var path := "$.dialogue.packages[%d]" % i
		var package: Variant = graph.packages[i]
		if not _object(package, ["index", "name", "metadata", "node_ids"], [], path):
			return false
		if (
			not _integer(package.index, path)
			or package.index != i
			or not _string(package.name, path)
		):
			return _fail("invalid_package", path)
		if not _dictionary(package.metadata, path) or not _ids(package.node_ids, path):
			return false
		for id: String in package.node_ids:
			if package_members.has(id) or not _nodes.has(id) or _nodes[id].package != i:
				return _fail("package_membership", path)
			package_members[id] = true
	if package_members.size() != _nodes.size():
		return _fail("package_membership", "$.dialogue.packages")
	var listed_pins: Dictionary = {}
	for node: Dictionary in graph.nodes:
		for field: String in ["parent", "speaker"]:
			if not _node_reference(node[field], "$.dialogue.nodes." + field, node.id):
				return false
		for field: String in ["input_pins", "output_pins"]:
			var direction := "input" if field == "input_pins" else "output"
			var pin_index := 0
			for id: String in node[field]:
				if listed_pins.has(id) or not _pins.has(id):
					return _fail("pin_membership", "$.dialogue.nodes")
				if _pins[id].owner != node.id or _pins[id].direction != direction:
					return _fail("pin_owner_mismatch", "$.dialogue.nodes")
				if _pins[id].index != pin_index:
					return _fail("pin_index_mismatch", "$.dialogue.pins")
				listed_pins[id] = true
				pin_index += 1
	if listed_pins.size() != _pins.size():
		return _fail("pin_membership", "$.dialogue.pins")
	if not _parent_cycles():
		return false
	for edge: Dictionary in graph.edges:
		if not _nodes.has(edge.source):
			return _fail("missing_edge_source", "$.dialogue.edges")
		if not _node_reference(edge.target, "$.dialogue.edges", edge.id):
			return false
		if not _pin_reference(edge.source_pin, edge.source, "$.dialogue.edges"):
			return false
		if not _pin_reference(edge.target_pin, edge.target, "$.dialogue.edges"):
			return false
		if edge.kind == "connection" and (edge.source_pin == null or edge.target_pin == null):
			return _fail("connection_requires_pins", "$.dialogue.edges")
		if edge.kind == "jump" and edge.source_pin != null:
			return _fail("jump_source_pin", "$.dialogue.edges")
	var choice_ids: Dictionary = {}
	for choice: Variant in graph.choices:
		if not _object(
			choice,
			["edge", "source", "source_pin", "target", "text_source"],
			["condition", "event"],
			"$.dialogue.choices"
		):
			return false
		for field: String in ["edge", "source", "source_pin", "target"]:
			if not _reference_id(choice[field], "$.dialogue.choices." + field):
				return false
		if not _edges.has(choice.edge) or choice_ids.has(choice.edge):
			return _fail("choice_edge", "$.dialogue.choices")
		var edge: Dictionary = _edges[choice.edge]
		if (
			edge.kind != "connection"
			or edge.source != choice.source
			or edge.source_pin != choice.source_pin
			or edge.target != choice.target
		):
			return _fail("choice_edge_mismatch", "$.dialogue.choices")
		if not _pins.has(choice.source_pin) or _pins[choice.source_pin].direction != "output":
			return _fail("choice_source_pin", "$.dialogue.choices")
		if not _enum(
			choice.text_source,
			["edge_label", "target_menu_text", "target_text", "none"],
			"$.dialogue.choices"
		):
			return false
		choice_ids[choice.edge] = true
	var hierarchy_ids: Dictionary = {}
	for entry: Variant in graph.hierarchy:
		if not _object(
			entry, ["id", "parent", "index", "depth", "properties"], [], "$.dialogue.hierarchy"
		):
			return false
		if not _nonzero_id(entry.id, "$.dialogue.hierarchy") or hierarchy_ids.has(entry.id):
			return _fail("duplicate_hierarchy_id", "$.dialogue.hierarchy")
		if (
			not _reference_id(entry.parent, "$.dialogue.hierarchy", true)
			or not _integer(entry.index, "$.dialogue.hierarchy")
			or not _integer(entry.depth, "$.dialogue.hierarchy")
		):
			return false
		if (
			not _dictionary(entry.properties, "$.dialogue.hierarchy")
			or not _node_reference(entry.id, "$.dialogue.hierarchy", entry.id)
		):
			return false
		hierarchy_ids[entry.id] = entry
	if not _hierarchy_structure(graph.hierarchy, hierarchy_ids):
		return false
	for warning: Variant in graph.warnings:
		if not _object(warning, ["kind", "source", "reference"], [], "$.dialogue.warnings"):
			return false
		if (
			not _enum(warning.kind, WARNING_KINDS, "$.dialogue.warnings")
			or not _id(warning.source, "$.dialogue.warnings", true)
			or not _id(warning.reference, "$.dialogue.warnings", true)
		):
			return false
	return _error.is_empty()


func _hierarchy_structure(entries: Array, by_id: Dictionary) -> bool:
	var next_sibling: Dictionary = {}
	for entry: Dictionary in entries:
		if entry.depth > 64:
			return _fail("hierarchy_depth_limit", "$.dialogue.hierarchy")
		if entry.parent == null:
			if entry.depth != 0:
				return _fail("hierarchy_depth_mismatch", "$.dialogue.hierarchy")
		else:
			if not by_id.has(entry.parent):
				return _fail("missing_hierarchy_parent", "$.dialogue.hierarchy")
			if by_id[entry.parent].depth + 1 != entry.depth:
				return _fail("hierarchy_depth_mismatch", "$.dialogue.hierarchy")
		if entry.index != next_sibling.get(entry.parent, 0):
			return _fail("hierarchy_index_mismatch", "$.dialogue.hierarchy")
		next_sibling[entry.parent] = entry.index + 1
		if _nodes.has(entry.id) and _nodes[entry.id].parent != entry.parent:
			_warning("hierarchy_parent_mismatch", "$.dialogue.hierarchy", entry.id, entry.parent)
	return _error.is_empty()


func _parent_cycles() -> bool:
	var done: Dictionary = {}
	for id: String in _nodes:
		var chain: Dictionary = {}
		var cursor: Variant = id
		while cursor != null and _nodes.has(cursor) and not done.has(cursor):
			if chain.has(cursor):
				return _fail("parent_cycle", "$.dialogue.nodes")
			chain[cursor] = true
			cursor = _nodes[cursor].parent
		done.merge(chain)
	return true


func _world(value: Variant) -> bool:
	if not _object(value, ["locations", "exits", "items"], [], "$.world"):
		return false
	for field: String in ["locations", "exits", "items"]:
		if not _array(value[field], "$.world." + field):
			return false
	for location: Variant in value.locations:
		if not _world_record(
			location,
			["id", "name"],
			["description", "dialogue", "condition", "event", "metadata"],
			"$.world.locations"
		):
			return false
		if not _string(location.name, "$.world.locations"):
			return false
		_locations[location.id] = true
	for item: Variant in value.items:
		if not _world_record(
			item,
			["id", "name", "location"],
			["description", "dialogue", "condition", "event", "metadata"],
			"$.world.items"
		):
			return false
		if (
			not _string(item.name, "$.world.items")
			or not _world_location(item.location, "$.world.items.location", true)
		):
			return false
	for exit_record: Variant in value.exits:
		if not _world_record(
			exit_record,
			["id", "from", "to"],
			["name", "condition", "event", "metadata"],
			"$.world.exits"
		):
			return false
		if (
			not _world_location(exit_record.from, "$.world.exits.from")
			or not _world_location(exit_record.to, "$.world.exits.to")
		):
			return false
		if exit_record.has("name") and not _string(exit_record.name, "$.world.exits.name"):
			return false
	return _error.is_empty()


func _world_record(value: Variant, required: Array, optional: Array, path: String) -> bool:
	if not _object(value, required, optional, path) or not _id(value.id, path):
		return false
	if _world_ids.has(value.id):
		return _fail("duplicate_world_id", path)
	_world_ids[value.id] = true
	if value.has("description") and not _string(value.description, path):
		return false
	if value.has("metadata") and not _dictionary(value.metadata, path):
		return false
	if value.has("dialogue"):
		if (
			not _reference_id(value.dialogue, path, true)
			or not _node_reference(value.dialogue, path, value.id, false)
		):
			return false
	return _declarative(value, path)


func _world_location(value: Variant, path: String, nullable := false) -> bool:
	if not _id(value, path, nullable):
		return false
	return (
		true
		if (nullable and value == null) or _locations.has(value)
		else _fail("missing_location", path)
	)


func _declarative(record: Dictionary, path: String) -> bool:
	if record.has("condition") and not _condition(record.condition, path + ".condition", 0):
		return false
	if record.has("event") and not _event(record.event, path + ".event"):
		return false
	if record.has("condition") and record.get("script") != null:
		_warning("declarative_condition_precedes_script", path)
	if record.has("event") and record.get("script") != null:
		_warning("declarative_event_precedes_script", path)
	return _error.is_empty()


func _variable_reference(value: Variant, path: String) -> Dictionary:
	if not _object(value, ["namespace", "name"], [], path):
		return {}
	if not _id(value.namespace, path) or not _id(value.name, path):
		return {}
	if not _variables.has(value.namespace) or not _variables[value.namespace].has(value.name):
		_fail("missing_variable", path)
		return {}
	return _variables[value.namespace][value.name]


func _condition(value: Variant, path: String, depth: int) -> bool:
	if depth > MAX_DSL_DEPTH:
		return _fail("condition_depth_limit", path)
	if typeof(value) != TYPE_DICTIONARY or not value.has("op"):
		return _fail("invalid_condition", path)
	match value.op:
		"eq", "ne":
			if not _object(value, ["op", "variable", "value"], [], path):
				return false
			var variable := _variable_reference(value.variable, path)
			return not variable.is_empty() and _typed_literal(value.value, variable.kind, path)
		"all", "any":
			if (
				not _object(value, ["op", "conditions"], [], path)
				or not _array(value.conditions, path)
			):
				return false
			if value.conditions.is_empty():
				return _fail("empty_condition_group", path)
			for child: Variant in value.conditions:
				if not _condition(child, path, depth + 1):
					return false
			return true
		"not":
			return (
				_object(value, ["op", "condition"], [], path)
				and _condition(value.condition, path, depth + 1)
			)
		_:
			return _fail("unsupported_condition", path)


func _event(value: Variant, path: String) -> bool:
	if typeof(value) != TYPE_DICTIONARY or not value.has("op"):
		return _fail("invalid_event", path)
	match value.op:
		"set":
			if not _object(value, ["op", "variable", "value"], [], path):
				return false
			var variable := _variable_reference(value.variable, path)
			return not variable.is_empty() and _typed_literal(value.value, variable.kind, path)
		"emit":
			return _object(value, ["op", "name"], ["payload"], path) and _id(value.name, path)
		_:
			return _fail("unsupported_event", path)
