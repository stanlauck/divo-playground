# SPDX-License-Identifier: MIT OR Apache-2.0
extends SceneTree

const Loader = preload("../addons/storyworld/storyworld_loader.gd")


func _initialize() -> void:
	var arguments := OS.get_cmdline_user_args()
	if arguments.size() != 2:
		print(
			(
				"Usage: godot --headless --path <project> --script examples/import.gd "
				+ "-- <input> <new-output.tres>"
			)
		)
		quit(2)
		return
	var output := arguments[1]
	if not output.ends_with(".tres") or FileAccess.file_exists(output):
		print("Output must be a new .tres file; existing files are not overwritten.")
		quit(2)
		return
	var result: Dictionary = Loader.new().import_file(arguments[0])
	if not result.ok:
		print("Import failed: %s at %s" % [result.error.code, result.error.path])
		quit(1)
		return
	if ResourceSaver.save(result.resource, output) != OK:
		print("Unable to save the Resource.")
		quit(1)
		return
	print("Imported validated data; warnings: %d" % result.resource.import_warnings.size())
	quit(0)
