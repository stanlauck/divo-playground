# PG-01: streaming prose importer

Rust crate for **FB2 / DOCX / UTF-8 TXT → rich chapter/block JSON**. No host
application dependency, network calls, external converters or ZIP extraction.
All fixtures and benchmark prose are synthetic.

## API

```rust
use pg_01_prose_importer::{import_fb2, Document, ImportOptions};
use std::io::Cursor;

let source = r#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
<body><section id="opening"><title><p>Opening</p></title>
<p>A <strong>bold</strong> beginning.</p></section></body></FictionBook>"#;
let mut json = Vec::new();
let report = import_fb2(Cursor::new(source), &mut json, &ImportOptions::default())?;
let document: Document = serde_json::from_slice(&json)?;
assert_eq!(report.words, 4);
assert_eq!(document.chapters[0].title.as_deref(), Some("Opening"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`import_fb2` and `import_txt` take `BufRead`; `import_docx` takes `Read + Seek`
because DOCX is a ZIP package. All take a `Write` destination and
`&ImportOptions`, and return `ImportReport`. Pass a buffered file/stdout writer
for production output. The importer does not flush a caller-owned writer.

JSON is emitted incrementally, not accumulated as a `Document`. Deserializing
it into `Document` is optional and naturally loads the complete output.
**On any error, discard partial JSON.** For atomic file publication, write to a
temporary file and rename it only after import and writer flush both succeed.

## JSON model, version 1

Top-level fields: `version: 1`, `source_format: "fb2" | "docx" | "txt"`,
`chapters`, `report`. Chapters have generated `id`, `level`, optional `title`,
`source_id`, `continuation_of`, and `blocks`.

| Block `type` | Fields |
|---|---|
| `paragraph` | `spans` |
| `heading` | `level`, `spans` |
| `list` | `id`, `ordered`, `start`, `number_format`, optional `marker`, `items` |
| `table` | `rows[].cells[]`: `column`, `colspan`, `rowspan`, `header`, `blocks` |
| `quote`, `epigraph` | `blocks`, `attribution` spans |
| `poem` | `title` spans, `epigraphs`, `stanzas[]` with `title` and separate `lines[][]`, `attribution` |
| `footnote` | `id`, `blocks` |

Each span contains `text` and optional `bold`, `italic`, `strike`,
`superscript`, `subscript`, `url`, `note_id`. False/absent marks are omitted.
Nested marks combine, and formatting boundaries never insert spaces.
Links are **data only**: they are neither fetched nor sanitized for browser
navigation. Consumers must apply their own URL-scheme policy.

### Format mapping

- **TXT:** UTF-8, optional BOM; blank lines separate paragraphs; internal
  line breaks and Unicode are preserved. No guessed headings or markup.
- **FB2:** namespace-qualified FictionBook 2.0, including UTF-8 and
  ASCII-compatible declared encodings such as Windows-1251. Titles/subtitles
  become headings, nested section depth becomes chapter/heading level.
  Chapters remain in reading order. Parent prose after a nested section creates
  a continuation with `continuation_of` pointing to the parent's first chapter.
  Citations, epigraphs, poems (distinct stanzas and verse lines), inline marks,
  links and table spans are preserved. `body name="notes"` / `"comments"`
  sections become separate footnotes; `a type="note"` links carry `note_id`.
- **DOCX:** transitional and strict WordprocessingML element namespaces.
  Reads conventional `word/document.xml`, styles, numbering, document
  relationships and footnotes (with their own relationships). Headings use
  outline levels or heading styles, including levels 1–6; inherited styles and
  explicit run overrides are honored. Leading unheaded prose uses a level-0
  chapter; subsequent prose stays in the current heading's chapter.
  Quote styles become quotes. Numbering IDs, levels, starting
  counters and marker templates are retained. Each list paragraph is one
  streamed `list` block; adjacent blocks with the same ID can be grouped by the
  consumer, without buffering an unbounded list. Tables retain header rows,
  horizontal and vertical grid merges. Footnote spans link to separate
  `docx-footnote-<id>` blocks in a trailing chapter.

FB2 levels can exceed six for deeply nested sections. Word outline levels up
to nine are retained instead of clamped. Table columns and list-item levels are
zero-based; heading/section levels are one-based (zero means unheaded prose).

## Loss report and limitations

`report` includes chapter count, recursive block count, whitespace-delimited
word count, sorted `{kind, occurrences}` losses, sorted `unresolved_footnotes`,
and `peak_block_bytes`. Word count includes headings, notes, table text, poem
titles/lines and attributions. Formatting changes within a word do not split it.
This is not linguistic CJK word segmentation.

Images/binary resources, font names/sizes, colors, layout, unsupported inline
marks and other skipped features are reported, not silently interpreted.
For example: `fb2.images`, `fb2.binary_resource`, `fb2.font_styles`,
`docx.images`, `docx.font_styles`, `docx.colors`, `docx.page_layout`.
Occurrences count skipped source constructs/resources, not unique visual
objects; an embedded image and its drawing reference can count separately.
FB2 book metadata and DOCX headers/footers, themes, comments, endnotes and
package metadata are not imported. DOCX uses visible field result text and
reports skipped field codes; it does not evaluate fields or generate numbering
glyphs. Deleted revisions are excluded; inserted text is retained with a loss
for revision metadata. No page-layout or round-trip fidelity is promised.
This is not a full FB2/OOXML schema validator.

## Streaming and limits

XML is read incrementally. A complex block (table, poem, quote or footnote) is
bounded and materialized as a unit; the complete main document is never loaded.
DOCX retains ZIP central-directory state and bounded styles/numbering/links.
FB2 retains a bounded section stack. Footnote IDs and loss categories use
bounded metadata. Processing memory does not grow with the number of plain
paragraphs; it does grow with bounded metadata and the largest complex block.

Default limits (all configurable in `ImportOptions`, all must be positive):

| Limit | Default |
|---|---|
| Input / DOCX archive and each document/footnote part | 256 MiB |
| Logical source tree, serialized JSON, or cumulative link-copy bytes for one block | 8 MiB |
| Each metadata part / expanded numbering / section-stack text / report ID and loss strings | 4 MiB |
| XML depth / style inheritance depth | 128 |
| ZIP entries, checked after central-directory parsing | 10,000 |
| Entries per metadata map/set | 100,000 |

XML tokens, including skipped text/attributes/comments, are bounded by the
larger block/metadata byte limit with 8 KiB reader slack. Oversized single
binary-text tokens are rejected rather than decoded or read into an unbounded
buffer. DOCTYPE/custom entities, unknown namespace prefixes, malformed XML,
illegal XML characters, invalid UTF-8 TXT and unsupported non-ASCII-compatible
XML encodings are rejected. Only builtin/numeric XML entities are accepted.
ZIP files are read directly by exact part names, never extracted.

`peak_block_bytes` is the largest logical source-tree byte count or serialized
block JSON byte count, **not allocator peak, total retained memory or RSS**.

## Offline examples and checks

Run from this package directory after dependencies have been fetched once:

```text
cargo run --offline --locked --example import -- fb2 samples/synthetic.fb2
cargo run --offline --locked --example import -- txt samples/synthetic.txt
cargo run --offline --locked --example make_docx -- target/synthetic.docx
cargo run --offline --locked --example import -- docx target/synthetic.docx
cargo test --offline --locked
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
cargo bench --offline --locked --bench import_200k
```

`make_docx` packages the checked-in synthetic XML parts into a complete sample
DOCX; it refuses to overwrite an existing output. Tests create ZIP fixtures in
memory, including malformed inputs and mocked IO failures.

The benchmark generates **40,000 five-word paragraphs = 200,000 words** for
each format. Input generation/compression is outside the timer; import, JSON
serialization and report construction are included, output goes to a sink.
Three runs per format are averaged (`PG01_BENCH_RUNS` overrides this). It asserts
exact word/block counts and reports timing and logical peak block bytes.

### Recorded run

2026-10-07, Windows, rustc 1.99.0, optimized bench profile, three runs per format:

| Format | Mean import time | Logical peak block bytes |
|---|---:|---:|
| TXT | 6.886 ms | 71 |
| FB2 | 21.855 ms | 72 |
| DOCX | 43.247 ms | 212 |

These synthetic sink-output timings are observations, not performance
guarantees or process-memory measurements.

## License

MIT OR Apache-2.0. See the repository's root license files.
