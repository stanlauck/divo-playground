# SPDX-License-Identifier: MIT OR Apache-2.0
@tool
class_name StoryworldResource
extends Resource

@export var version: int = 1
@export var world: Dictionary = {}
@export var dialogue: Dictionary = {}
@export var import_warnings: Array[Dictionary] = []


func to_document() -> Dictionary:
	return {
		"version": version, "world": world.duplicate(true), "dialogue": dialogue.duplicate(true)
	}


func get_location(id: String) -> Dictionary:
	return _find(world.get("locations", []), "id", id)


func get_exit(id: String) -> Dictionary:
	return _find(world.get("exits", []), "id", id)


func get_item(id: String) -> Dictionary:
	return _find(world.get("items", []), "id", id)


func get_dialogue_node(id: String) -> Dictionary:
	return _find(dialogue.get("nodes", []), "id", id)


func get_dialogue_pin(id: String) -> Dictionary:
	return _find(dialogue.get("pins", []), "id", id)


func get_dialogue_edge(id: String) -> Dictionary:
	return _find(dialogue.get("edges", []), "id", id)


func get_dialogue_choice(edge_id: String) -> Dictionary:
	return _find(dialogue.get("choices", []), "edge", edge_id)


func get_variable(namespace_name: String, variable_name: String) -> Dictionary:
	for variable: Dictionary in dialogue.get("variables", []):
		if variable["namespace"] == namespace_name and variable["name"] == variable_name:
			return variable.duplicate(true)
	return {}


func get_node_declarative(id: String) -> Dictionary:
	return _declarative(get_dialogue_node(id))


func get_choice_declarative(edge_id: String) -> Dictionary:
	return _declarative(get_dialogue_choice(edge_id))


func _find(records: Array, key: String, id: String) -> Dictionary:
	for record: Dictionary in records:
		if record.get(key) == id:
			return record.duplicate(true)
	return {}


func _declarative(record: Dictionary) -> Dictionary:
	# This is the authoritative declarative contract, never an interpreter.
	return {"condition": record.get("condition"), "event": record.get("event")}.duplicate(true)
