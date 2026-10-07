# l10n-exchange (PG-14)

Converts localization string tables between JSON, XLIFF 2.1 and CSV. Each line keeps its context, character name, translator note, length limit and review state.

Licence: MIT OR Apache-2.0.

## String table JSON

```json
{
  "version": 1,
  "source_lang": "en",
  "target_lang": "de",
  "strings": [
    {
      "id": "ui.button.continue",
      "source": "Continue",
      "target": "Weiter",
      "context": "Main menu button",
      "character": "Narrator",
      "note": "Imperative, no punctuation",
      "max_length": 12,
      "state": "final"
    }
  ]
}
```

| Field | Required | Meaning |
|---|---|---|
| `version` | yes | Always `1`. |
| `source_lang`, `target_lang` | source only | BCP 47 tags. `target_lang` is required once any entry has a `target`. |
| `id` | yes | Unique. ASCII letters, digits, `.`, `-`, `_` and `:` (a subset of XML NMTOKEN). |
| `source` | yes | Source text. Whitespace and newlines are kept. |
| `target` | no | Translation. |
| `context` | no | Where or how the line is used. |
| `character` | no | Name of the speaking character. |
| `note` | no | Free-form note for translators. |
| `max_length` | no | Maximum target length in Unicode code points. |
| `state` | no | `initial`, `translated`, `reviewed` or `final` (the XLIFF 2.1 segment states). |

Unknown fields are rejected. An empty string counts as a missing field in every format. Text must be valid in XML 1.0, so control characters other than tab, LF and CR are rejected.

## Mapping

| String table | XLIFF 2.1 | CSV column |
|---|---|---|
| `source_lang` / `target_lang` | `<xliff srcLang trgLang>` | passed as arguments |
| entry | `<unit id>` with one `<segment>` | one row |
| `source` / `target` | `<source>` / `<target>` | `source` / `target` |
| `context` | `<note category="context">` | `context` |
| `character` | `<note category="character">` | `character` |
| `note` | `<note category="comment">` | `note` |
| `max_length` | `slr:sizeRestriction`, profile `xliff:codepoints` | `max_length` |
| `state` | `<segment state>` | `state` |

The writers always produce the same output for the same input. JSON → XLIFF → CSV → JSON is lossless; `samples/` contains one table in all three formats, and a test checks that those files match the writers.

The XLIFF reader also accepts files from other tools:

- XLIFF 2.0 and 2.1, several `<file>` elements, and nested `<group>` elements, flattened in document order.
- A unit with several segments becomes one entry. Texts are joined, `<ignorable>` parts are kept, and the state becomes the least advanced segment state. A unit without targets gets no `target`.
- Notes in other categories, or with no category, are joined into `note`. Several notes in the same category are joined with newlines.
- Errors are returned for inline markup (`<ph>`, `<pc>`, `<mrk>` and similar), for size profiles other than `xliff:codepoints`, for XLIFF 1.x and for DTDs.

The CSV writer follows RFC 4180: a header row, UTF-8 without a BOM, and CRLF line endings. The reader accepts a BOM, columns in any order, and only `id` and `source` are required. Short rows are padded with empty cells. Unknown columns, duplicate columns and rows with extra cells are rejected.

## Usage

```rust
use l10n_exchange::{from_json, to_xliff, to_csv, from_csv};

let table = from_json(&std::fs::read_to_string("samples/strings.json")?)?;
let xliff = to_xliff(&table)?;
let csv = to_csv(&table)?;
let back = from_csv(csv.as_bytes(), "en", Some("de"))?;
assert_eq!(back, table);

for v in table.length_violations() {
    eprintln!("{} is {} code points, limit {}", v.id, v.actual, v.max_length);
}
```

To convert files from the command line (formats are picked by file extension):

```sh
cargo run --example convert -- samples/strings.json out.xlf
cargo run --example convert -- out.csv out.json en de   # CSV input needs languages
```

## Tests

```sh
cargo test
```

Tests run offline and use only the synthetic data in `samples/` and `tests/`.
