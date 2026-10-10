# SPDX-License-Identifier: MIT OR Apache-2.0
"""Best-effort UE startup registration; failures never block editor startup."""

try:
    import unreal  # type: ignore[import-not-found]

    def _import_storyworld_menu() -> None:
        try:
            unreal.log("Storyworld Importer: call storyworld_importer.import_storyworld(path)")
        except Exception:
            pass

    try:
        menus = unreal.ToolMenus.get()
        menu = menus.extend_menu("LevelEditor.MainMenu.Tools")
        entry = unreal.ToolMenuEntry(type=unreal.MultiBlockType.MENU_ENTRY, name="StoryworldImport")
        entry.set_label("Storyworld → Import JSON...")
        entry.set_tool_tip("Import a neutral Storyworld JSON file")
        entry.set_string_command(unreal.ToolMenuStringCommandType.PYTHON, "", "_import_storyworld_menu()")
        menu.add_menu_entry("", entry)
        menus.refresh_all_widgets()
    except Exception:
        pass
except Exception:
    pass
