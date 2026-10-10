<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# PG-15 Unreal Engine 5 Storyworld importer

This Python-only editor plugin imports the PG-04 neutral Storyworld JSON v1
document into the current Unreal level. It creates one labeled placeholder
actor for every location, exit, and item, and returns a deterministic import
report. The implementation targets Unreal Engine **5.3+** Python Editor
Scripting APIs (`EditorActorSubsystem`, `unreal.Vector`, and `unreal.Paths`).

The package copies PG-04's `schema.json` and both synthetic samples into this
directory. It has no runtime or build-time dependency on PG-04.

## Install and use

Copy `StoryworldImporter/` to `<Project>/Plugins/`, enable **Python Editor
Script Plugin**, and open a level. Run `init_unreal.py` from the editor Python
console (or configure it as an editor startup script). The guarded startup
script adds **Tools → Storyworld → Import JSON...**; projects may call the
Python API directly when a file picker is available:

```python
from storyworld_importer import import_storyworld
report = import_storyworld("/path/to/world.storyworld", delete_orphans=False)
print(report.to_json())
```

For headless import, use the exact command below from the project directory:

```sh
UnrealEditor-Cmd <Project>.uproject -run=pythonscript \
  -script="<Project>/Plugins/StoryworldImporter/Content/Python/import_storyworld_cli.py" \
  -- "/path/to/world.storyworld" --dry-run
```

Add `--delete-orphans` to destroy actors that no longer occur in the input.
Without it, removed records are reported and kept in the level.

## JSON v1 recap

The complete copied shape is in [`schema.json`](schema.json). Both `version`
tokens are the integer `1` (a JSON `1.0` is rejected). Structural fields are
closed; arbitrary extra data belongs in `metadata`.

| Area | Required fields | Optional fields / references |
|---|---|---|
| Root | `version`, `world`, `dialogue` | none |
| Location | `id`, `name` | `description`, `dialogue`, `condition`, `event`, `metadata` |
| Exit | `id`, `from`, `to` | `name`, `condition`, `event`, `metadata` |
| Item | `id`, `name`, `location` | `description`, `dialogue`, `condition`, `event`, `metadata` |
| Dialogue | PG-03 graph v1 fields | node/choice declarative `condition` and `event` |

World IDs share one uniqueness domain. Exit endpoints, item locations, and
world dialogue links must resolve. Dialogue graph references, package/pin
ownership, hierarchy cycles, and the PG-04 declarative DSL are validated.
Filtered PG-03 references are retained as warnings, as in PG-04.

## Identity, layout, and dialogue storage

Every actor receives the stable tags `storyworld:id=<id>` and
`storyworld:kind=location|exit|item`. A canonical JSON record is base64 encoded
in `storyworld:data=<base64>`, and the dialogue file path is stored in a
`storyworld:dialogue=` tag. Re-import finds actors by these tags, updates their
label, transform, tags, and metadata, and never spawns a duplicate. Ambiguous
or duplicate identity tags produce a report error before edits are made.

Locations use a deterministic grid, 1000 Unreal units apart (eight columns per
row). Items sit near their location; unplaced items use the origin. Exits sit
at the midpoint of their endpoint locations. The default placeholder is
`unreal.Actor`; callers can pass a configured class to `apply_document`. A
class with a `TextRenderComponent` may display the record name; plain actors
remain ordinary Outliner placeholders.

The complete dialogue graph is written as UTF-8 JSON to
`<Project>/Saved/Storyworld/<input-stem>.dialogue.json`. No DataAsset or runtime
loader is required; the actor tag is the reference.

## Reports

`ImportReport.to_json()` emits stable UTF-8 JSON with sorted arrays and keys:

| Field | Meaning |
|---|---|
| `created` | IDs spawned during this import |
| `updated` | IDs whose record or placement changed |
| `unchanged` | IDs already matching the plan |
| `orphans` | Existing IDs absent from the input |
| `warnings` | Non-fatal filtered references or storage notes |
| `errors` | Objects with `code`, `path`, and `message` |

## Limits and decisions

Input is capped at 8 MiB, JSON depth at 64, values at 500,000, decoded strings
at 1 MiB, identifiers at 128 UTF-8 bytes, structural arrays at 50,000, and
declarative nesting at 32. Duplicate keys, invalid UTF-8/BOM use, NULs,
lone surrogates, non-finite numbers, and integers outside signed int64 are
rejected. File reads are bounded before parsing.

The importer stores and validates dialogue; it does not execute scripts,
conditions, or events. Dialogue is saved beside the level through the project
`Saved` directory because it is the smallest DataAsset-free mechanism and is
portable to command-line editor sessions.

## Not supported

This plugin does not generate authored scene geometry, runtime gameplay,
Blueprints, navmeshes, or interpreted dialogue. It does not make network calls.
Actor deletion is opt-in with `delete_orphans=True`.

## Offline tests

Unreal is unavailable in CI, so tests install `tests/fake_unreal.py` into
`sys.modules` and exercise the same application code offline. Run from this
package directory:

```sh
python3 -m unittest discover -s tests -t .
python3 -m compileall -q StoryworldImporter tests
```

The suite currently contains 37 tests and has no third-party dependencies.

## License

MIT OR Apache-2.0. Samples are synthetic. See `LICENSE-MIT` and
`LICENSE-APACHE`.
