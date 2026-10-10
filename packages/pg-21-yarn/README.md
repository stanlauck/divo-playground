<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# PG-21 — Yarn Spinner 2 parser

`pg-21-yarn` parses synthetic or project `.yarn` files without evaluating Yarn
expressions. It preserves dialogue, speakers, line ids, hashtags, interpolation,
markup, commands, choices, conditions, jumps, declarations, source locations,
and a separate diagnostic report.

The graph is an exact local copy of the PG-03 `DialogueGraph` v1 model in
`src/model.rs`; this crate has no build dependency on PG-03. Only the fields of
that model are emitted, so a strict PG-03 reader can deserialize the result.

## Mapping

| Yarn construct | PG-03 v1 representation |
| --- | --- |
| `title:` block | `flow_fragment` / `Node`; title and headers in `properties` |
| dialogue line | `dialogue_fragment` / `Line`; speaker, `properties.line_id`, `properties.tags` |
| consecutive `->` options | `hub` / `OptionGroup`, `Option` children, pin edges and `choices[]` |
| `<<jump Target>>` | `jump` / `Jump`, `properties.target`, pin-to-pin jump edge |
| `<<stop>>` | `jump` / `Stop`, target `stop`, no outgoing edge |
| `<<if>>` chain | `condition` / `If`; branch labels `if`, `elseif:<expr>`, `else`; branch statements carry `properties.clause` |
| `<<set>>` or other command | `instruction` / `Set` or `Command`, Yarn instruction script and command properties |
| `<<declare>>` | `variable_namespaces` and `variables`; adjacent `///` text is the description |

Every node has exactly one input and one output pin. Sequential flow, choice
candidates, branch entries, fall-through, and jumps reference those pins. A
container connects to its first statement, including a first choice hub, so all
options remain reachable. Dynamic jump expressions are retained without a
static missing-node finding.

Node ids are derived from source titles and nested ordinals. A global issued-id
set prevents collisions; duplicate titles receive an unused `~2`, `~3`, …
suffix and a report finding. Edge ids are `e0`, `e1`, … in reading order.

## Diagnostics and CLI

`parse_sources_with_report` and `read_files_with_report` return
`(DialogueGraph, ParseReport)`. The graph contains only PG-03 `warnings[]`
(`missing_node`, `unknown_type`, `unknown_variable_type`, and related PG-03
variants). File, line, column, finding code, severity, and message are retained
in `ParseReport` and are never added to the graph.

```console
yarn2graph <in.yarn>... [-o out.json] [--report report.json] [--strict]
```

`-o` writes the graph instead of stdout. `--report` writes the separate report.
`--strict` treats warnings as failures. Exit code 0 means accepted input, 1
means findings above the threshold, and 2 means usage, I/O, UTF-8, or hard-limit
failure. Tests never use the network.

## Decisions and limits

Headers and source-specific values are retained in maps. `#line:` is stored as
`properties.line_id`, other hashtags as `properties.tags`, and Yarn markup and
interpolations remain literal. Explicit declaration types win when compatible;
`as float` therefore remains a float even for an integer literal. `///` comments
are cleared at node boundaries and only attach to the immediately following
`<<declare>>`. Trailing `//` comments after commands are ignored.

`ParseOptions` bounds each input at 8 MiB, total nodes at 200,000, findings at
10,000, and nesting at 64 by default. File reads enforce the byte bound while
reading, including pipes/devices. Malformed constructs produce bounded findings
and best-effort output.

## Library example

```rust
use pg_21_yarn::{parse_sources_with_report, ParseOptions, Source};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let (graph, report) = parse_sources_with_report(
    &[Source::new("demo.yarn", "title: Start\n---\nAda: Hello. #line:1\n")],
    &ParseOptions::default(),
)?;
assert_eq!(graph.version, 1);
assert!(report.findings.is_empty());
# Ok(())
# }
```

## Checks

```console
cargo fmt --all -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --locked --offline
```

## Licence

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
