# pg-13-gexf-writer

Small Rust library for writing validated GEXF 1.3 graphs with typed node and
edge attributes, dynamic attribute values, and temporal topology spells.

The input model is a serializable Rust API. Synthetic JSON examples are in
`samples/typed_dynamic_graph.json` and `samples/typed_static_graph.json`.

## Usage

```rust
use pg_13_gexf_writer::Graph;

let graph: Graph = serde_json::from_str(include_str!(
    "../samples/typed_dynamic_graph.json"
))?;
let mut xml = Vec::new();
graph.write_to(&mut xml)?;
assert!(String::from_utf8(xml)?.contains("version=\"1.3\""));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Add `pg-13-gexf-writer` and `serde_json` to your application's dependencies to
deserialize the sample. Or construct `Graph` directly through the Rust API.
`Graph::validate` can be called independently before writing.

Supported GEXF attribute types are `integer`, `long`, `double`, `float`,
`boolean`, `string`, and `liststring`. Static and dynamic attributes can coexist
in one class; the writer groups definitions by class and mode.
Lists use the GEXF 1.3 bracketed syntax, such as `[north, south]` or `[]`.
To keep parsing unambiguous, list items must be nonempty and trimmed, without
`|`, `,`, `;`, brackets, quotes, or backslashes.

Dynamic graphs require a `time_format` (`integer`, `double`, `date`, or
`date_time` in the Rust/JSON API). The writer emits `dateTime` in GEXF XML.
Intervals use inclusive bounds; omitted bounds represent infinity.
Exclusive bounds were removed in GEXF 1.3. Legacy `start_open` / `end_open`
JSON fields are rejected rather than silently changing their meaning.
Numeric, ISO date (`YYYY-MM-DD`, years 0001 to 9999), and ISO date-time endpoints
(with up to nine fractional-second digits) are validated and ordered before
XML is written. Date-times accept no timezone, `Z`, or numeric offsets.
Repeated values for one dynamic attribute may use distinct, non-overlapping
intervals; unbounded or overlapping repetitions are rejected. Touching
inclusive endpoints count as overlap.
Empty graphs still include the required `nodes` and `edges` containers.

## Checks

Run tests offline:

```text
cargo test --offline
```

Render the synthetic sample to GEXF XML:

```text
cargo run --offline --example write_sample
```

Render a different JSON graph (run from this package directory):

```text
cargo run --offline --example write_sample -- samples/typed_static_graph.json
```

All samples are synthetic. Tests make no network calls.

## License

MIT OR Apache-2.0. See the repository's `LICENSE` and `LICENSE-APACHE` files.
