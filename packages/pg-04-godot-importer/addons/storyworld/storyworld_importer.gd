# SPDX-License-Identifier: MIT OR Apache-2.0
@tool
extends EditorImportPlugin

const Loader = preload("storyworld_loader.gd")


func _get_importer_name() -> String:
	return "storyworld.neutral.v1"


func _get_visible_name() -> String:
	return "Storyworld neutral data"


func _get_recognized_extensions() -> PackedStringArray:
	return PackedStringArray(["storyworld"])


func _get_save_extension() -> String:
	return "tres"


func _get_resource_type() -> String:
	return "Resource"


func _get_preset_count() -> int:
	return 1


func _get_preset_name(_preset_index: int) -> String:
	return "Validated data only"


func _get_import_options(_path: String, _preset_index: int) -> Array[Dictionary]:
	return []


func _get_format_version() -> int:
	return 1


func _can_import_threaded() -> bool:
	return false


func _import(
	source_file: String,
	save_path: String,
	_options: Dictionary,
	_platform_variants: Array[String],
	_gen_files: Array[String]
) -> Error:
	var result: Dictionary = Loader.new().import_file(source_file)
	if not result.ok:
		push_error("Storyworld import failed: %s at %s" % [result.error.code, result.error.path])
		return ERR_PARSE_ERROR
	return ResourceSaver.save(result.resource, save_path + ".tres")
