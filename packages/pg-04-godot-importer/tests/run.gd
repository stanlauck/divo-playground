# SPDX-License-Identifier: MIT OR Apache-2.0
extends SceneTree

const Loader = preload("../addons/storyworld/storyworld_loader.gd")
const Decoder = preload("../addons/storyworld/strict_json.gd")
const Data = preload("../addons/storyworld/storyworld_resource.gd")

var _passed := 0
var _failed := 0
var _assertions := 0
var _base: Dictionary = {}


func _initialize() -> void:
	_base = _decode_file("res://samples/pg03-compatible.storyworld")
	_test("PG-03 exact dialogue preservation", _pg03_compatibility)
	_test("declarative additions and script precedence", _declarative_additions)
	_test("typed Resource .tres save/load", _resource_roundtrip)
	_test("automatic storyworld importer and ordinary JSON exclusion", _editor_import)
	_test("strict JSON grammar and UTF-8", _strict_json)
	_test("signed int64 and Unicode preservation", _precision)
	_test("world references, IDs and versions", _world_rejections)
	_test("dialogue topology, fields and variables", _dialogue_rejections)
	_test("DSL references, literal types and operations", _dsl_rejections)
	_test("partial PG-03 graph and warnings", _partial_graph)
	_test("all-original metadata, scripts and getter isolation", _data_only)
	_test("minimal world and dialogue", _minimal)
	_test("decoder and record budgets", _budgets)
	_test("IO diagnostics do not disclose paths or content", _io_errors)
	_test("lossless numeric and control-string Resource roundtrip", _full_precision_roundtrip)
	_test("PG-03 name, opaque type and localization compatibility", _pg03_edge_types)
	_test("hierarchy, zero references and pin indices", _hierarchy_rejections)
	_test("nested DSL and exotic op/value types", _dsl_depth_and_types)
	print("PG-04: %d tests passed, %d assertions, %d failed" % [_passed, _assertions, _failed])
	quit(1 if _failed > 0 else 0)


func _test(name: String, action: Callable) -> void:
	var before := _failed
	action.call()
	if before == _failed:
		_passed += 1
		print("PASS: ", name)


func _check(condition: bool, name: String) -> void:
	_assertions += 1
	if not condition:
		_failed += 1
		print("FAIL: ", name)


func _decode_file(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	var result: Dictionary = Decoder.new().decode(file.get_buffer(file.get_length()))
	file.close()
	if not result.ok:
		_check(false, "decode synthetic fixture")
		return {}
	return result.data


func _import(document: Dictionary) -> Dictionary:
	# JSON.stringify is deliberately not used for int64 edge-case tests.
	return Loader.new().import_bytes(JSON.stringify(document, "", false, true).to_utf8_buffer())


func _reject(document: Dictionary, code: String) -> void:
	var result := _import(document)
	_check(
		not result.ok and result.get("error", {}).get("code") == code, "expected rejection " + code
	)
	_check(not result.has("resource"), "no partial Resource on error")


func _pg03_compatibility() -> void:
	var result := Loader.new().import_file("res://samples/pg03-compatible.storyworld")
	_check(result.ok, "base imports")
	if not result.ok:
		print(result)
		return
	var graph := _decode_file("res://samples/pg03-neutral.json")
	_check(result.resource.dialogue == graph, "dialogue exactly PG-03 output")
	_check(result.resource.world == _base.world, "world preserved")
	_check(result.resource.get_location("room").id == "room", "stable world ID")
	_check(
		result.resource.get_dialogue_node("0x0100000000000003").id == "0x0100000000000003",
		"stable PG-03 ID"
	)
	_check(result.resource.get_dialogue_node("0X0100000000000003").is_empty(), "case-sensitive ID")
	_check(result.resource.import_warnings.size() == 4, "opaque scripts each warn")


func _declarative_additions() -> void:
	var result := Loader.new().import_file("res://samples/declarative.storyworld")
	_check(result.ok, "extended imports")
	if not result.ok:
		print(result)
		return
	var node: Dictionary = result.resource.get_dialogue_node("0x0100000000000006")
	_check(node.script.text == "Story.visited", "ArticyScript unchanged")
	_check(node.condition.op == "all", "declarative condition preserved")
	_check(
		result.resource.get_node_declarative(node.id).event.op == "emit",
		"declarative authoritative contract"
	)
	_check(
		result.resource.get_choice_declarative("edge-1").event.op == "set", "choice event preserved"
	)
	var codes: Array = []
	for warning: Dictionary in result.resource.import_warnings:
		codes.append(warning.code)
	_check("declarative_condition_precedes_script" in codes, "condition precedence warned")
	_check("declarative_event_precedes_script" in codes, "event precedence warned")


func _resource_roundtrip() -> void:
	var result := Loader.new().import_file("res://samples/declarative.storyworld")
	DirAccess.make_dir_recursive_absolute("res://target")
	var path := "res://target/roundtrip.tres"
	_check(ResourceSaver.save(result.resource, path) == OK, "save typed .tres")
	var loaded: Resource = ResourceLoader.load(path, "", ResourceLoader.CACHE_MODE_IGNORE)
	_check(loaded is Data, "typed Resource loads")
	_check(loaded.to_document() == result.resource.to_document(), "roundtrip all data")
	_check(loaded.import_warnings == result.resource.import_warnings, "roundtrip warnings")
	_check(loaded.get_variable("Story", "visited").value == false, "set events not executed")
	_check(loaded.get_variable("Story", "coins").value == 3, "script instructions not executed")
	DirAccess.remove_absolute(path)


func _editor_import() -> void:
	var remap := ConfigFile.new()
	_check(remap.load("res://samples/declarative.storyworld.import") == OK, "editor remap exists")
	_check(
		remap.get_value("remap", "importer") == "storyworld.neutral.v1",
		"our editor importer handles storyworld"
	)
	var imported: Resource = ResourceLoader.load(
		"res://samples/declarative.storyworld", "", ResourceLoader.CACHE_MODE_IGNORE
	)
	_check(imported is Data, "real editor-imported Resource loads")
	if imported is Data:
		_check(imported.get_exit("door").to == "yard", "editor import contains world")
	_check(
		not FileAccess.file_exists("res://samples/pg03-neutral.json.import"),
		"ordinary .json not imported"
	)


func _strict_json() -> void:
	for json: String in [
		'{"a":1,"a":2}',
		'{"a":1,}',
		"[1,]",
		'{"a":01}',
		'{"a":NaN}',
		'{"a":1e}',
		'{"a":1.}',
		'{"a":+1}',
		'"line\nbreak"',
		'"\\ud800"',
		'"\\udc00"',
		'"\\u0000"',
		"true false"
	]:
		_check(not Decoder.new().decode(json.to_utf8_buffer()).ok, "reject malformed JSON")
	for bytes: PackedByteArray in [
		PackedByteArray([0xc0, 0x80]),
		PackedByteArray([0xed, 0xa0, 0x80]),
		PackedByteArray([0xf4, 0x90, 0x80, 0x80]),
		PackedByteArray([0x22, 0, 0x22]),
		PackedByteArray([0xf0, 0x9f])
	]:
		_check(not Decoder.new().decode(bytes).ok, "reject invalid UTF-8")
	_check(
		Decoder.new().decode(PackedByteArray([0xef, 0xbb, 0xbf]) + "{}".to_utf8_buffer()).ok,
		"input BOM accepted"
	)
	_check(not Decoder.new().decode("[".repeat(66).to_utf8_buffer()).ok, "depth bound")


func _precision() -> void:
	for pair: Array in [
		["9223372036854775807", 9223372036854775807],
		["-9223372036854775808", -9223372036854775808],
		["9007199254740993", 9007199254740993]
	]:
		var parsed: Dictionary = Decoder.new().decode(pair[0].to_utf8_buffer())
		_check(
			parsed.ok and typeof(parsed.data) == TYPE_INT and parsed.data == pair[1], "exact int64"
		)
	for json: String in [
		"9223372036854775808",
		"-9223372036854775809",
		"1e9999",
		"1e99999999999999999999999999999999999",
		"1e-99999999999999999999999999999999999"
	]:
		_check(not Decoder.new().decode(json.to_utf8_buffer()).ok, "number range rejection")
	var text := '"\\ud83d\\ude80 e\\u0301 Север 南"'
	_check(
		Decoder.new().decode(text.to_utf8_buffer()).data == "🚀 e\u0301 Север 南",
		"surrogate pair and original combining mark"
	)


func _world_rejections() -> void:
	var document := _base.duplicate(true)
	document.version = 2
	_reject(document, "unsupported_version")
	document = _base.duplicate(true)
	document.dialogue.version = 2
	_reject(document, "unsupported_version")
	document = _base.duplicate(true)
	document.world.exits[0].to = "absent"
	_reject(document, "missing_location")
	document = _base.duplicate(true)
	document.world.items[0].id = "room"
	_reject(document, "duplicate_world_id")
	document = _base.duplicate(true)
	document.world.locations[0].dialogue = "missing-node"
	_reject(document, "missing_dialogue_node")
	document = _base.duplicate(true)
	document.world.locations[0].extra = true
	_reject(document, "unknown_field")


func _dialogue_rejections() -> void:
	var document := _base.duplicate(true)
	document.dialogue.nodes[1].id = document.dialogue.nodes[0].id
	_reject(document, "duplicate_id")
	document = _base.duplicate(true)
	document.dialogue.pins[0].owner = document.dialogue.nodes[1].id
	_reject(document, "pin_owner_mismatch")
	document = _base.duplicate(true)
	document.dialogue.edges[0].target_pin = document.dialogue.pins[3].id
	_reject(document, "pin_owner_mismatch")
	document = _base.duplicate(true)
	document.dialogue.nodes[0].parent = document.dialogue.nodes[2].id
	_reject(document, "parent_cycle")
	document = _base.duplicate(true)
	document.dialogue.choices[0].target = document.dialogue.nodes[1].id
	_reject(document, "choice_edge_mismatch")
	document = _base.duplicate(true)
	document.dialogue.variables[1].value = true
	_reject(document, "variable_type_mismatch")
	document = _base.duplicate(true)
	document.dialogue.packages[0].node_ids.pop_back()
	_reject(document, "package_membership")


func _dsl_rejections() -> void:
	var document := _base.duplicate(true)
	document.world.exits[0].condition.variable.name = "missing"
	_reject(document, "missing_variable")
	document = _base.duplicate(true)
	document.world.exits[0].condition.value = "False"
	_reject(document, "variable_type_mismatch")
	document = _base.duplicate(true)
	document.world.exits[0].condition.op = "execute"
	_reject(document, "unsupported_condition")
	document = _base.duplicate(true)
	document.dialogue.nodes[0].condition = {
		"op": "eq", "variable": {"namespace": "Story", "name": "coins"}, "value": true
	}
	_reject(document, "variable_type_mismatch")
	document = _base.duplicate(true)
	document.dialogue.choices[0].event = {
		"op": "set", "variable": {"namespace": "Story", "name": "missing"}, "value": 1
	}
	_reject(document, "missing_variable")
	document = _base.duplicate(true)
	document.world.exits[0].condition = {"op": "all", "conditions": []}
	_reject(document, "empty_condition_group")


func _partial_graph() -> void:
	var document := _base.duplicate(true)
	document.dialogue.edges[0].target = "partial-target"
	document.dialogue.edges[0].target_pin = "partial-pin"
	document.dialogue.choices[0].target = "partial-target"
	var result := _import(document)
	_check(result.ok, "PG-03 filtered graph accepted")
	if result.ok:
		_check(
			result.resource.dialogue.edges[0].target == "partial-target",
			"missing reference retained"
		)
		_check(result.resource.import_warnings.size() > 4, "missing references warned")


func _data_only() -> void:
	var result := Loader.new().import_file("res://samples/pg03-compatible.storyworld")
	var resource: Resource = result.resource
	var node: Dictionary = resource.get_dialogue_node("0x0100000000000003")
	node.text = "changed copy"
	_check(
		resource.get_dialogue_node(node.id).text != node.text, "getter returns detached dictionary"
	)
	_check(
		resource.get_location("room").metadata.marker == "res://this-is-data-not-a-loaded-asset",
		"paths are data only"
	)
	_check(
		resource.get_dialogue_pin("0x0100000000000012").script.text == "Story.visited = true;",
		"pin script text preserved"
	)


func _minimal() -> void:
	var document := _base.duplicate(true)
	document.world = {"locations": [], "exits": [], "items": []}
	for key: String in document.dialogue:
		if typeof(document.dialogue[key]) == TYPE_ARRAY:
			document.dialogue[key] = []
	var result := _import(document)
	_check(result.ok, "empty valid v1 imports")
	_check(result.resource.import_warnings.is_empty(), "empty graph has no warnings")


func _budgets() -> void:
	var result: Dictionary = Decoder.new().decode(
		" ".repeat(Decoder.MAX_BYTES + 1).to_utf8_buffer()
	)
	_check(not result.ok and result.error.code == "input_limit", "8 MiB input cap")
	result = Decoder.new().decode(
		('"' + "a".repeat(Decoder.MAX_STRING_BYTES + 1) + '"').to_utf8_buffer()
	)
	_check(not result.ok and result.error.code == "string_limit", "1 MiB decoded string cap")
	result = Decoder.new().decode(("[" + "0,".repeat(Decoder.MAX_VALUES) + "0]").to_utf8_buffer())
	_check(not result.ok and result.error.code == "value_limit", "500k value cap")
	result = Decoder.new().decode(("0." + "1".repeat(Decoder.MAX_NUMBER_BYTES)).to_utf8_buffer())
	_check(not result.ok and result.error.code == "number_token_limit", "float token cap")
	var document := _base.duplicate(true)
	document.dialogue.definitions = []
	document.dialogue.definitions.resize(50_001)
	_reject(document, "record_limit")


func _io_errors() -> void:
	var result := Loader.new().import_file("res://target/no-such-sensitive-source.storyworld")
	_check(
		not result.ok and result.error == {"code": "io_error", "path": "$"}, "sanitized IO error"
	)
	result = Loader.new().import_bytes(
		'{"secret": "DO_NOT_INCLUDE_THIS", "secret": 1}'.to_utf8_buffer()
	)
	_check(not result.ok and result.error.code == "duplicate_key", "duplicate keys fail")
	_check(not str(result.error).contains("DO_NOT_INCLUDE_THIS"), "diagnostic excludes raw text")
	result = Loader.new().import_bytes('{"a":1,"\\u0061":2}'.to_utf8_buffer())
	_check(not result.ok and result.error.code == "duplicate_key", "decoded key collisions")


func _full_precision_roundtrip() -> void:
	var source := FileAccess.get_file_as_string("res://samples/pg03-compatible.storyworld")
	var added_metadata := (
		'"NumericEdgeCases": [9223372036854775807, -9223372036854775808, '
		+ "9007199254740993, 1.2345678901234567, 2.2250738585072014e-308, "
		+ "1.7976931348623157e308, 5e-324, -0.0, 2.0], "
		+ '"Controls": "\\b\\f\\n\\r\\t \\" \\\\", "Project": {'
	)
	source = source.replace('"Project": {', added_metadata)
	var result := Loader.new().import_bytes(source.to_utf8_buffer())
	_check(result.ok, "edge values import without rounding ints")
	if not result.ok:
		return
	var path := "res://target/numbers.tres"
	_check(ResourceSaver.save(result.resource, path) == OK, "edge values save")
	var loaded: Resource = ResourceLoader.load(path, "", ResourceLoader.CACHE_MODE_IGNORE)
	_check(loaded is Data, "edge values load")
	var original: Array = result.resource.dialogue.metadata.NumericEdgeCases
	var restored: Array = loaded.dialogue.metadata.NumericEdgeCases
	for i in range(original.size()):
		_check(typeof(original[i]) == typeof(restored[i]), "numeric Variant type retained")
		_check(original[i] == restored[i], "numeric value retained exactly")
	_check(
		loaded.dialogue.metadata.Controls == result.resource.dialogue.metadata.Controls,
		"controls and escapes retained"
	)
	DirAccess.remove_absolute(path)


func _pg03_edge_types() -> void:
	var document := _base.duplicate(true)
	document.dialogue.text_mode = "localization_keys"
	document.dialogue.warnings = [{"kind": "localization_keys", "source": null, "reference": null}]
	document.dialogue.nodes[2].text = "SYNTHETIC.LOCALIZATION.KEY"
	document.dialogue.variables[0].name = "0x000"
	document.dialogue.variable_namespaces[0].variables[0] = "0x000"
	document.world.exits[0].condition.variable.name = "0x000"
	document.dialogue.variables[3].kind = "other"
	document.dialogue.variables[3].value = {"unmapped": [true, null, 5]}
	var result := _import(document)
	_check(result.ok, "zero-like variable name and unknown value retained")
	if result.ok:
		_check(
			result.resource.dialogue == document.dialogue,
			"unmapped and localization data preserved"
		)
	document.world.items[0].event.variable.name = document.dialogue.variables[3].name
	document.world.items[0].event.value = {}
	_reject(document, "variable_type_mismatch")


func _hierarchy_rejections() -> void:
	var document := _base.duplicate(true)
	document.dialogue.nodes[0].parent = "0x000"
	_reject(document, "zero_id")
	document = _base.duplicate(true)
	document.dialogue.pins[0].index = 2
	_reject(document, "pin_index_mismatch")
	document = _base.duplicate(true)
	document.dialogue.hierarchy[0].depth = 2
	_reject(document, "hierarchy_depth_mismatch")
	document = _base.duplicate(true)
	document.dialogue.hierarchy[1].parent = "absent-entry"
	_reject(document, "missing_hierarchy_parent")
	document = _base.duplicate(true)
	document.dialogue.hierarchy[1].depth = 50
	_reject(document, "hierarchy_depth_mismatch")
	document = _base.duplicate(true)
	document.dialogue.hierarchy[1].index = 10
	_reject(document, "hierarchy_index_mismatch")
	document = _base.duplicate(true)
	document.dialogue.hierarchy[0].parent = document.dialogue.hierarchy[1].id
	_reject(document, "hierarchy_depth_mismatch")
	document = _base.duplicate(true)
	document.dialogue.hierarchy[0].depth = 100
	_reject(document, "hierarchy_depth_limit")


func _dsl_depth_and_types() -> void:
	var document := _base.duplicate(true)
	var condition: Dictionary = document.world.exits[0].condition
	for i in range(33):
		condition = {"op": "not", "condition": condition}
	document.world.exits[0].condition = condition
	_reject(document, "condition_depth_limit")
	for op: Variant in [true, [], {}, 1, null]:
		document = _base.duplicate(true)
		document.world.exits[0].condition.op = op
		_reject(document, "unsupported_condition")
	document = _base.duplicate(true)
	document.world.items[0].event.op = "run"
	_reject(document, "unsupported_event")
	document = _base.duplicate(true)
	document.world.items[0].event.variable = "Story.coins"
	_reject(document, "expected_object")
	document = _base.duplicate(true)
	document.world.items[0].event.variable.name = "pace"
	document.world.items[0].event.value = 9007199254740993
	var source := JSON.stringify(document, "", false, true).replace(
		"9007199254740992", "9007199254740993"
	)
	var result := Loader.new().import_bytes(source.to_utf8_buffer())
	_check(
		not result.ok and result.error.code == "lossy_float_literal",
		"int cannot silently round as float"
	)
