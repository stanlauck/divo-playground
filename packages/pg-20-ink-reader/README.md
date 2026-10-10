<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# ink-reader

`ink-reader` reads compiled Ink runtime JSON (Ink format versions 19–21) and produces the PG-03 neutral `DialogueGraph` v1 plus a separate unsupported-feature report. It implements the runtime container format locally and does not depend on an Ink crate or execute story code.

## API and CLI

```rust
use ink_reader::{read_compiled, ReadOptions};
let (graph, report) = read_compiled(std::io::Cursor::new(json_bytes), &ReadOptions::default())?;
```

`ink2graph story.ink.json [-o graph.json] [--report report.json] [--strict]` writes deterministic pretty JSON. `--strict` exits 1 when warnings or unsupported features are present.

The reader uses the public [Ink runtime JSON format](https://raw.githubusercontent.com/inkle/ink/master/Documentation/ink_JSON_runtime_format.md), consulted on 2026-10-10. Fixtures are hand-written synthetic JSON and `.ink` reference text.

## Supported mapping

| Ink runtime construct | Neutral graph result |
|---|---|
| Named root containers | `flow_fragment` Knot; named child containers are Stitch |
| `^text`, newline, glue | Line nodes; glue joins the current run |
| Choice points (`*`, `flg`) | ChoicePoint hub, Option nodes, decoded flags, `choices[]` |
| `->` and DONE/END | Divert jump nodes and resolved jump edges |
| Conditional divert shape | Condition/script when a conditional object is encountered |
| `VAR=` / `temp=` | Set/Temp instruction with rendered expression |
| `#` tags | Adjacent line `properties.tags` |

Every node has one input and output pin, and sequential/container-entry edges use the PG-03 pin conventions. IDs are deterministic source paths with collision suffixes. Global declarations become namespace `ink` variables when an initial value is statically available.

## Report

```json
{"version":1,"ink_version":20,"unsupported":[{"feature":"tunnels","count":1,"paths":["knot.2"]}],"warnings":[{"kind":"unknown_type","path":"knot.2","message":"..."}],"stats":{"knots":1,"stitches":0,"lines":1,"choices":0,"diverts":0,"variables":0}}
```

Unsupported elements are preserved as `instruction` nodes with `source_type: "Unsupported"` and `properties.raw`; parsing continues. Tunnels, functions, external calls, lists/listDefs, variable diverts/pointers, threads, sequences/cycles/shuffles and unknown objects are reported. Missing divert targets produce `missing_node` warnings. Conditional branch labels are a static heuristic because runtime JSON does not retain all source-level clause labels.

## Limits and decisions

Defaults are 32 MiB input, depth 256, and one million JSON elements. Callers may lower these through `ReadOptions`; malformed JSON, unsupported Ink versions, non-array roots and limit violations return typed errors. Output ordering follows source order and report feature ordering is sorted for deterministic bytes. This crate contains a local copy of the PG-03 v1 model to avoid a build dependency.

Run checks with `cargo test --locked`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.

### Input fields

The top-level object requires `inkVersion` (19, 20, or 21) and `root` (the compiled container array). `listDefs` is optional and must be an object; non-empty definitions are retained as unsupported list nodes. Container arrays contain ordered runtime values and may end in a metadata object with `#n`, `#f`, and named child containers. Values include `^` strings, `\\n`, glue, numbers, control command strings, native operators, divert/choice/variable objects, and nested arrays.
