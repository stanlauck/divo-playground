# SPDX-License-Identifier: MIT OR Apache-2.0
@tool
extends RefCounted

const Decoder = preload("strict_json.gd")
const Validator = preload("validator.gd")
const Data = preload("storyworld_resource.gd")


func import_bytes(bytes: PackedByteArray) -> Dictionary:
	var decoded: Dictionary = Decoder.new().decode(bytes)
	if not decoded.ok:
		return decoded
	var checked: Dictionary = Validator.new().validate(decoded.data)
	if not checked.ok:
		return checked
	var resource = Data.new()
	resource.version = decoded.data.version
	resource.world = decoded.data.world.duplicate(true)
	resource.dialogue = decoded.data.dialogue.duplicate(true)
	resource.import_warnings.assign(checked.warnings)
	return {"ok": true, "resource": resource}


func import_file(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return {"ok": false, "error": {"code": "io_error", "path": "$"}}
	var length := file.get_length()
	if length > Decoder.MAX_BYTES:
		file.close()
		return {"ok": false, "error": {"code": "input_limit", "path": "$"}}
	var bytes := file.get_buffer(length + 1)
	file.close()
	if bytes.size() != length:
		return {"ok": false, "error": {"code": "input_changed", "path": "$"}}
	return import_bytes(bytes)
