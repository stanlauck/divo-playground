# pg-13-gexf-writer

Small Rust library for writing validated GEXF 1.3 graphs with typed node and
edge attributes, dynamic attribute values, and temporal topology spells.

The input model is a serializable Rust API. The complete synthetic JSON shape
is in `samples/typed_dynamic_graph.json`.

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
`boolean`, `string`, and `liststring`. Attributes of the same class must share
one mode (`static` or `dynamic`), as required by the GEXF attributes container.
List items are separated by `|`; to keep the encoding unambiguous, items
containing `|`, `,`, or `;` are rejected.

Dynamic graphs require a `time_format` (`integer`, `double`, `date`, or
`date_time` in the Rust/JSON API). The writer emits `dateTime` in GEXF XML.
Intervals allow omitted bounds and optional open bounds. Numeric, ISO date, and
ISO date-time endpoints (with up to nine fractional-second digits) are
validated and ordered before XML is written.
Repeated values for one dynamic attribute may use distinct, non-overlapping
intervals; unbounded or overlapping repetitions are rejected.

## Checks

Run tests offline:

```text
cargo test --offline
```

Render the synthetic sample to GEXF XML:

```text
cargo run --offline --example write_sample
```

All samples are synthetic. Tests make no network calls.

## License

MIT OR Apache-2.0. See the repository's `LICENSE` and `LICENSE-APACHE` files.
