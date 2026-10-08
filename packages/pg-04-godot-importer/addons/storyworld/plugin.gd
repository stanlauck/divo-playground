# SPDX-License-Identifier: MIT OR Apache-2.0
@tool
extends EditorPlugin

const Importer = preload("storyworld_importer.gd")
var _importer: EditorImportPlugin


func _enter_tree() -> void:
	_importer = Importer.new()
	add_import_plugin(_importer)


func _exit_tree() -> void:
	if _importer != null:
		remove_import_plugin(_importer)
		_importer = null
