# PG-04: Storyworld importer for Godot 4

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

A standalone, data-only editor add-on. Version-1 neutral JSON describes
locations, exits, items, and an embedded **PG-03 `DialogueGraph` v1**.
The importer saves a typed `StoryworldResource` as `.tres`, with stable,
case-sensitive IDs, original dialogue data, and separate import warnings.

Tested with **Godot 4.7.2 stable**. No native extension, runtime dependency,
scene generation, network access, asset loading, or script interpreter.
Older Godot releases are not supported by this package's test contract.

## Install and use

1. Copy `addons/storyworld/` into a Godot project.
2. Enable **Storyworld neutral JSON importer** in Project Settings → Plugins.
3. Add a `*.storyworld` file containing the JSON document described below.

Only `.storyworld` is registered for automatic editor import. Ordinary `.json`
files are untouched. `ResourceLoader.load("res://world.storyworld")` returns the
imported typed Resource after the editor has imported it. Keep the add-on
scripts with the project, including when exporting a game.

```gdscript
const Loader = preload("res://addons/storyworld/storyworld_loader.gd")

# Explicit API accepts .json too, without registering a JSON importer.
var result: Dictionary = Loader.new().import_file("res://world.json")
if result.ok:
    var world: StoryworldResource = result.resource
    var room: Dictionary = world.get_location("room")
    var gate: Dictionary = world.get_dialogue_node("0x0100000000000006")
    var declarations: Dictionary = world.get_node_declarative(gate.id)
    ResourceSaver.save(world, "res://world.tres")
else:
    push_error("%s at %s" % [result.error.code, result.error.path])
```

`import_bytes(PackedByteArray)` has the same result shape. A success contains
`{ok: true, resource: StoryworldResource}`; a failure contains
`{ok: false, error: {code, path, offset?}}` and **no partial Resource**.
Paths are structural JSON paths, never source filenames. Decoder offsets are
zero-based Unicode scalar positions after an optional leading UTF-8 BOM.
Pre-decode errors use offset zero.

The Resource exports `version`, `world`, `dialogue`, and `import_warnings`.
`to_document()` and lookup methods return detached copies:

- `get_location`, `get_exit`, `get_item`
- `get_dialogue_node`, `get_dialogue_pin`, `get_dialogue_edge`
- `get_dialogue_choice(edge_id)`, `get_variable(namespace, name)`
- `get_node_declarative`, `get_choice_declarative`

Missing record lookups return `{}`. Declarative getters return
`{condition: null, event: null}` when absent. Exported Resource dictionaries
remain editable Godot data; they are not immutable or a live variable store.

### Explicit command-line import

From this package's directory (create the output directory first):

```sh
godot --headless --editor --path . --import
godot --headless --path . --script examples/import.gd -- samples/declarative.storyworld new-world.tres
```

The example refuses to overwrite an existing `.tres`. The editor uses Godot's
normal import cache and reimport lifecycle instead.

## JSON v1

See [`schema.json`](schema.json) and
[`samples/declarative.storyworld`](samples/declarative.storyworld).
The root has exactly `version`, `world`, and `dialogue`. Both version fields
must be the **integer token `1`**, not `1.0` or a string.

`world` contains the required arrays `locations`, `exits`, and `items`.
Empty arrays and an empty PG-03 graph are valid.

| Record | Required fields | Optional fields |
|---|---|---|
| Location | `id`, `name` | `description`, `dialogue`, `condition`, `event`, `metadata` |
| Exit | `id`, `from`, `to` | `name`, `condition`, `event`, `metadata` |
| Item | `id`, `name`, `location` | `description`, `dialogue`, `condition`, `event`, `metadata` |

`from`, `to`, and non-null `location` refer to existing locations.
An item's `location: null` means it is not placed. A record's optional
`dialogue` is null or an existing dialogue-node ID.
All world record IDs share one uniqueness domain, separate from dialogue IDs.
Names/descriptions are strings; `metadata` is an arbitrary JSON object.
Unknown structural fields are rejected; additional data belongs in metadata.

### PG-03 compatibility

The entire `dialogue` object uses the field names, enums and arrays from
`packages/pg-03-articy-reader/src/model.rs`. In particular:

- Nodes, input/output pins, connection/jump edges and choice edge IDs are
  separate, not collapsed into a new simplified dialogue model.
- Package membership, pin ownership/direction/index, choice-edge agreement,
  nonzero node/pin IDs, parent cycles and hierarchy structure are checked.
- Missing referenced dialogue nodes/pins are retained and warned, matching
  PG-03's filtered-export behavior. Edge sources and world links must exist.
- Hierarchy parent disagreement with a node is warned, not rewritten.
- Localization keys, BBCode, unknown node/variable kinds, definitions,
  raw variable values, properties, templates, metadata and existing PG-03
  warnings are retained unchanged.

The only schema additions are optional **`condition` and `event` on dialogue
nodes and choices**. They use the same DSL as world records below.
No PG-03 package changes are needed.

`samples/pg03-neutral.json` is the actual PG-03 strict-reader output for its
hand-authored `samples/synthetic.articy.json`. The baseline
`samples/pg03-compatible.storyworld` embeds that output unchanged.
Every saved/reloaded `.tres` retains parsed data values and numeric Variant
types, not original JSON whitespace, object-key order or numeric spelling.

### Declarative conditions and events

Conditions:

```json
{"op":"eq","variable":{"namespace":"Story","name":"visited"},"value":false}
{"op":"ne","variable":{"namespace":"Story","name":"coins"},"value":0}
{"op":"all","conditions":[{"op":"eq","variable":{"namespace":"Story","name":"visited"},"value":false}]}
{"op":"any","conditions":[{"op":"eq","variable":{"namespace":"Story","name":"coins"},"value":3}]}
{"op":"not","condition":{"op":"eq","variable":{"namespace":"Story","name":"visited"},"value":true}}
```

Events:

```json
{"op":"set","variable":{"namespace":"Story","name":"coins"},"value":4}
{"op":"emit","name":"door_opened","payload":{"source":"room"}}
```

Variable references must resolve to PG-03 variables. Boolean, integer, float
and string literals must match that variable's kind. A float accepts finite
float tokens or integer tokens within ±(2^53−1). Integer variables require
signed int64 integer tokens. `other` values are retained but cannot participate
in `eq`, `ne` or `set`. `all`/`any` must contain at least one condition.
`emit` needs a nonempty name and may carry arbitrary JSON payload data.
There are no coercions, assignments inside conditions, expressions, loops,
code strings, interpolation, lists of events, or unknown operations.

**The importer validates and stores this DSL; it does not evaluate conditions,
change variables, emit runtime events, or run ArticyScript.** Declarative
fields are the authoritative contract for consumers. When a node also has
`script`, that original script remains reference-only, and precedence warnings
identify the declarative addition. `get_node_declarative()` and
`get_choice_declarative()` expose only the authoritative declarative fields.
Scripts on pins and original nodes always receive `opaque_script_not_executed`
warnings, even without a declarative replacement. Scripts inherited through
an edge's pins are still opaque; availability/branch selection is not computed.

## Strict input and limits

Godot's built-in JSON parser is intentionally lenient and represents numbers
as floats. This add-on instead uses a bounded UTF-8/JSON decoder:

- Reject invalid/overlong UTF-8, surrogate scalars, lone JSON surrogates,
  duplicate **decoded** object keys, comments, raw controls, trailing commas,
  invalid number grammar and trailing content.
- Accept a leading BOM, surrogate pairs, combining marks and escaped
  JSON controls without Unicode normalization.
- Preserve signed int64 tokens, including numbers beyond 2^53.
  Unsigned integers above `9223372036854775807` are explicitly rejected.
- Preserve finite IEEE-754 floats. Float tokens are at most 128 bytes and
  their explicit exponent must be in −400…308; nonfinite results are rejected.
  Decimal rounding/underflow follows Godot's IEEE-754 conversion.
- Reject NUL explicitly instead of allowing Godot to substitute U+FFFD.
  Strings containing `\u0000` are not representable by this package.

| Budget | Limit |
|---|---|
| Input | 8 MiB |
| JSON nesting | 64 child steps from root |
| JSON values | 500,000, including containers |
| Decoded string | 1 MiB UTF-8 |
| Each structural array / generated warnings | 50,000 |
| Identifier | 128 UTF-8 bytes, nonempty, trimmed, no C0/C1 controls |
| Declarative nesting | 32 child steps from its root |
| Hierarchy depth | 64 |

Input is materialized within those bounds, not streamed. File reads are bounded
even if a file grows. These budgets are independent of PG-03's larger defaults.
The JSON Schema validates shape; byte budgets, duplicate keys, lexical numeric
types, typed variable references and graph integrity are enforced by GDScript.
JSON-shaped raw fields never trigger resource loading or code execution.

## Offline validation

Install optional developer tools once; the tests themselves never access the
network. Python 3.10+ and the pinned versions in `requirements-dev.txt` are used
only for schema tests, formatting, and the test runner.

```sh
python -m pip install -r requirements-dev.txt
python tests/run.py
python tests/run.py --godot /path/to/godot
```

Without `--godot`/`GODOT_BIN`, the runner explicitly reports engine checks
as skipped. With Godot 4.7.2 stable it performs a real headless editor import,
loads the `.storyworld` import result, and runs the GDScript tests. The runner
fails on engine error/warning output **even if Godot exits zero**.

Coverage includes strict JSON/UTF-8 and budgets; PG-03 equality;
filtered/localized exports; graph references and hierarchy; typed DSL and
precedence; detached lookups; int64/double/control-string `.tres` round trips;
and proof that neither scripts nor `set`/`emit` run during import/load.
The included synthetic corpus and schema are reproducible:

```sh
python examples/make_schema.py
cargo run --offline --manifest-path ../pg-03-articy-reader/Cargo.toml --example read -- ../pg-03-articy-reader/samples/synthetic.articy.json --strict
python examples/make_samples.py samples/pg03-neutral.json new-sample-directory
```

The generators write the schema to stdout or create a **new** sample directory;
they do not overwrite the included oracle. Godot caches/UIDs, temporary test
Resources and Python caches are ignored. No engine binary is included.

## License

MIT OR Apache-2.0. All samples are synthetic. See `LICENSE-MIT` and
`LICENSE-APACHE`.
