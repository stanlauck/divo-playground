# PG-03: native articy:draft JSON reader

Independent Rust crate: **single-file native articy:draft JSON → versioned
neutral dialogue graph**. Retains models, containers, pins, connections, jumps,
branch candidates, variables, hierarchy, templates and unknown metadata.
No script interpreter, network access, asset access or host-application coupling.
All checked-in fixture content is hand-authored and synthetic.

## API

```rust
use pg_03_articy_reader::{read_export, ImportOptions, NodeKind};
use std::io::Cursor;

let source = r#"{"Packages":[{"Name":"Synthetic","Models":[
  {"Type":"DialogueFragment","Properties":{"Id":"line","Text":"A quiet field."}}
]}]}"#;
let graph = read_export(Cursor::new(source), &ImportOptions::default())?;
assert_eq!(graph.nodes[0].kind, NodeKind::DialogueFragment);
assert_eq!(graph.nodes[0].text.as_deref(), Some("A quiet field."));
let output_json = serde_json::to_string_pretty(&graph)?;
assert!(output_json.contains("dialogue_fragment"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`read_export<R: Read>(reader, &options) -> Result<DialogueGraph>` reads a bounded
UTF-8 JSON input (optional BOM). Unlike PG-01, this reader is **not streaming**:
it materializes the input and graph to resolve forward references and hierarchy.
No output is returned on validation failure. The caller controls JSON writing
and must handle writer/flush errors.

## Accepted input

The native `Packages`/`Models` export layout, associated with
`Settings.ExportVersion: "1.0"`:

- Required `Packages[]`: `Name` and `Models[]`.
- Required model `Type` and `Properties.Id`.
- Optional `ObjectDefinitions[]`: `Type`, `Class`, optional `InheritsFrom`;
  custom-template model types resolve through these definitions.
- Embedded `Properties.InputPins[]` / `OutputPins[]`: `Id`, `Owner`, optional
  `Text`, `Connections[]` with `Target`, `TargetPin`, optional `Label`.
- Optional `GlobalVariables[]`: `Namespace`, `Variables[]` with `Variable`,
  `Type`, `Value`, optional `Description`.
- Optional `Hierarchy`: recursive `Id`, `Children[]`, with other fields retained.
- `Settings`, `Project`, `ScriptMethods`, `Assets` and other top-level metadata
  are retained, not interpreted as executable code or resource locations.

Absent/null optional pin/connection/definition/variable arrays mean empty.
An empty `Packages` array is valid. This is structural compatibility, not a
blanket promise for every articy:draft product version. Export-version metadata
is retained but not used as a guessed product-version number. Generic Engine
ZIP archives, separate engine-plugin files and custom export-rule layouts
without these structural fields are not supported.

Source layout references:
[official JSON export help](https://www.articy.com/help/adx/Exports_JSON.html),
[official pins/connections help](https://www.articy.com/help/adx/Flow_PinsConnections.html),
and the public [native schema reference](https://github.com/scenarioworld/articy-js/blob/main/src/json.ts).
Only field/layout facts were used; no third-party story, code or fixture is
included in this package. No articy:draft executable is needed for tests.

## Neutral JSON, version 1

| Field | Meaning |
|---|---|
| `version` | `1` |
| `text_mode` | `literal` or `localization_keys` |
| `metadata`, `definitions` | Root metadata and raw object definitions |
| `packages` | Package index/name/metadata, source-order node IDs |
| `nodes` | Every model, with source ID/type, normalized kind, parent, text, menu text, speaker, script, pin IDs, properties/template/metadata |
| `pins` | Source ID, owner, input/output direction, source-order index, optional script, remaining properties |
| `edges` | Connections/jumps, generated ID, source/target model and pin IDs, source-order connection index, label and raw connection properties |
| `choices` | Branch candidates referencing an edge/source pin/target and their text source |
| `variable_namespaces`, `variables` | Namespace metadata, names, declared types, parsed values and original values |
| `hierarchy` | Flattened preorder entries with parent, depth and sibling index |
| `warnings` | Typed warning with source/reference IDs, never full dialogue text |

Model/pin IDs are nonempty opaque strings, retained **exactly**, not converted
to numbers or case-normalized. Lookup is case-sensitive. Model and pin IDs
share a global uniqueness domain; duplicates are rejected, not deduplicated.
Missing/null references and all-zero hexadecimal references mean no reference;
actual object/pin IDs cannot be the zero sentinel.

Built-in flow/dialogue/entity/folder/asset kinds are recognized. Other models
remain `other`, never dropped. Raw model properties are retained except embedded
pins; pin properties are retained except connections. Templates and
model/package-level unknown fields survive.

### Topology and choices

Edges preserve `Target` / `TargetPin` records **on every pin**, including
container input pins. No assumption that every edge is output-to-input is
made. Jump targets become separate `jump` edges with no source pin.
Connections keep source array order, including parallel edges and self-loops.
Generated `edge-1`, `edge-2`, … IDs are deterministic for the same input order,
not stable after insertions/reordering, and never replace source object IDs.

`choices` lists output-pin connections when the source model has multiple
connections or the target has nonempty `MenuText`. These are **candidates**,
not a claim that the player can select them. Conditions, Hub behavior and
condition-node true/false output selection are not evaluated. The text source
is `edge_label`, `target_menu_text`, `target_text` or `none`; consumers retrieve
the string from the referenced edge/target. Large target texts are not copied
once per incoming edge. No graph traversal engine or inferred start node exists.

### Scripts, variables and localization

- Condition/Instruction `Expression` is stored as ArticyScript text.
- Input-pin `Text` is a condition; output-pin `Text` is an instruction.
- Expressions, assignments, external function calls and branch availability
  are **never executed, parsed or rewritten**.
- Boolean/Integer/Float/String variable types accept their native string
  initial values (or matching JSON scalars). Integer values use signed 64-bit
  range; floats must be finite. Unknown types preserve the original JSON value
  and produce a warning. Names are unique within each namespace; empty
  namespaces are retained.
- Rich text/BBCode/engine formatting is preserved as text, not converted to
  HTML or sanitized. Consumers must escape it for their own rendering context.
- `set_Localization: True` annotates `text_mode: localization_keys` and warns:
  keys are retained, and no translation spreadsheet/resource is loaded.

## Validation and limits

Malformed JSON/UTF-8, duplicate JSON keys at any depth, invalid structural
types, conflicting pin owners, target/pin ownership mismatches, ID collisions,
cyclic model parents and cyclic type inheritance are errors. Flow cycles are
valid. Missing referenced models, parents, speakers, pins and hierarchy objects
are retained with warnings for filtered/partial exports. Set
`strict_references: true` to make missing/inconsistent references errors.
Structurally invalid references are always errors.

Default configurable limits:

| Limit | Default |
|---|---:|
| Input bytes | 32 MiB |
| Models / variables / warnings | 100,000 each |
| Pins / edges | 400,000 each |
| Hierarchy entries | 200,000 |
| JSON / hierarchy / type-inheritance depth | 64 |
| ID / type / variable-name bytes | 128 |

All limits must be positive. Serde JSON's built-in recursion ceiling remains
enabled in addition to the configured depth. The byte bound is not a process
RSS cap: JSON/object/map allocations and retained source properties add
overhead. JSON numbers use serde_json's normal signed/unsigned 64-bit integer
or finite IEEE-754 floating-point representation; arbitrary-precision metadata
numbers are not promised.

## Offline examples and validation

From this package directory, after dependencies have been fetched once:

```text
cargo run --offline --locked --example read -- samples/synthetic.articy.json
cargo run --offline --locked --example read -- samples/synthetic.articy.json --strict
cargo test --offline --locked
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
```

The CLI writes neutral JSON to stdout only after the graph validates. Tests
build small synthetic exports in memory, include malformed input/IO mocks,
and use no network calls or real exported story text.

## License

MIT OR Apache-2.0. See the repository's root license files.
