# PG-02: screenplay PDF importer

Independent Rust crate, MIT OR Apache-2.0. Import a **text-layer PDF** into
versioned neutral screenplay JSON: classified blocks, original extracted lines,
page/coordinate provenance, and a doubtful-lines report. English and Russian
screenplay markers are recognized. No host-application dependency.

## Library

```rust
use pg_02_screenplay_importer::{import_pdf, ElementKind, ImportOptions};
use std::io::Cursor;

let pdf = include_bytes!("../samples/synthetic.pdf");
let screenplay = import_pdf(Cursor::new(pdf.as_slice()), &ImportOptions::default())?;
assert_eq!(screenplay.version, 1);
assert_eq!(screenplay.pages.len(), 2);
assert_eq!(screenplay.blocks[0].kind, ElementKind::SceneHeading);
let neutral_json = serde_json::to_string_pretty(&screenplay)?;
assert!(neutral_json.contains("scene_heading"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`import_pdf<R: Read>(reader, &options) -> Result<Screenplay>` materializes the
bounded input and result. This is **not a streaming importer**. Writer/flush
errors belong to the caller. An error returns no partial screenplay.

The pure-Rust [hayro interpreter](https://github.com/LaurenzV/hayro) supplies PDF
syntax, font/CMap decoding and positional glyphs. No PDF reader executable,
system-font discovery, subprocess, network request or OCR service is used.
Dependencies must be fetched once before offline builds.

## Input scope and unsupported content

- Text operators in pages and nested Form XObjects, including inherited resources.
- Unicode from `ToUnicode` mappings and the backend's supported font encodings.
  Missing mappings produce a replacement character and an explicit doubt.
- Standard fonts use the backend's embedded permissive substitutes. Geometry
  and fallback Unicode may differ from a particular PDF viewer.
- Invisible text layers are extracted, including pre-existing OCR text.
  The package does not perform or verify OCR itself.
- Image-only/scanned pages, outlines and other nontext content are not converted
  to screenplay. Such pages have `unsupported_no_text_layer` plus
  `no_extractable_text`; a blank page is reported separately.
- Text+image pages retain their text and report `image_content_not_imported`.
  Images/attachments are not exported. Raster image pixels are not requested
  by the extraction device.
- PDF JavaScript, launch actions, URLs and embedded files are never executed or
  opened. Annotation appearance streams are disabled, not screenplay text.
- Password/decryption failures are explicit errors. No password option is
  provided; the backend can read PDFs accessible with an empty password.

This is best-effort **text-layer extraction**, not a promise of complete visual
fidelity for every PDF. Font decoding is experimental in the backend. Partial
extraction/fallback-resource warnings taint that page's lines as uncertain
rather than assigning them a confident screenplay role. Unknown operators and
missing named resources are reported. Malformed PDFs may be repaired or partly
skipped by the backend; not every such repair is exposed as a warning.

## Neutral JSON, version 1

| Field | Meaning |
|---|---|
| `version` | `1` |
| `coordinate_system` | `crop_rotated_top_left_points` |
| `pages` | 1-based number, displayed width/height, rotation, source UserUnit, status, image count, source-line IDs |
| `lines` | Source-line ID/page/index/row, earliest glyph draw order, text/raw text, approximate bounds/baseline/font size, inferred-space count, extraction issues, suggested kind/confidence |
| `blocks` | Generated ID, kind, text, source-line IDs, scene-heading and character-cue block references, confidence |
| `doubts` | Line/block references, suggested kind, alternatives and machine-readable reasons |
| `warnings` | Page number and machine-readable extraction/loss reason |

Kinds: `scene_heading`, `action`, `character`, `dialogue`, `parenthetical`,
`transition`, `unknown`. Confidence is ordinal `low`, `medium`, `high`, **not a
calibrated probability**. Every extracted line belongs to exactly one block,
including uncertain lines and page furniture. Doubts contain references, not
another copy of the story text.

`line-1` / `block-1` IDs are deterministic for unchanged input/settings, not
stable after insertions, changed fonts or different backend versions. A block's
`scene` points to its preceding scene-heading block; `speaker` points to its
character-cue block, never a guessed actor/entity ID. A scene-heading block
itself has no parent scene. Blocks do not merge across pages.

### Text and coordinates

- Coordinates are PDF points in the displayed, intersected CropBox/MediaBox
  after page rotation and UserUnit scaling, origin at the top left, +X right
  and +Y down. UserUnit must be finite, positive and at most 75,000.
- Bounds are **approximate font-em/advance rectangles**, not precise visible
  outlines or true typographic ascender/descender metrics. Type3 glyphs use a
  nominal 600-unit advance for bounds. The baseline and font size come from the
  glyph transform. Coordinates are finite floating-point numbers.
- Lines sort top-to-bottom then left-to-right. Nearby baselines form rows;
  large horizontal gaps split row segments and flag possible multiple columns.
- `raw_text` concatenates the decoded glyph strings in that geometric order
  before inferred spaces. It is **not original PDF bytes**, an operator stream
  or a lossless reading-order reconstruction.
- `text` adds a space for a likely word gap. The count is retained. Block text
  trims surrounding whitespace and joins wrapped lines with a newline. Source
  strings retain their whitespace and are not case/Unicode-normalized.
- Fill+stroke callbacks for the same glyph are coalesced; arbitrary overprints
  are not generally deduplicated.
- Vertical, reversed or materially slanted text and multiple columns are kept
  as `unknown` with doubts. The package is not a general column/RTL layout engine.
  Clipping/visibility fidelity is not guaranteed.
- Consumers must escape text for their rendering context. No HTML sanitization,
  formatting markup conversion or text execution is performed.

### Classification and doubts

Deterministic heuristics combine lexical markers, indentation, adjacent lines
and dialogue state. Supported scene prefixes include `INT.`, `EXT.`,
`INT/EXT.`, `INT./EXT.`, `I/E.`, `ИНТ.`, `НАТ.`, `ЭКСТ.`, combined Russian
prefixes and an optional leading scene number. Transitions include `CUT TO:`,
`DISSOLVE TO:`, `FADE IN/OUT`, `СКЛЕЙКА`, `ЗАТЕМНЕНИЕ`, `НАПЛЫВ`.

Normal screenplay layout is assumed, not required. Character cues, dialogue,
wrapped parentheticals and action paragraphs use position/context. Title-case
or isolated cues are uncertain. Repeated marginal headers/footers and marginal
page numbers stay in the source as `unknown`, never silently dropped.

The report flags ambiguous uppercase text, orphan dialogue/parentheticals,
incomplete headings, uncertain cues, columns/orientation, missing Unicode,
control characters, fallback/unsupported backend content and possible page
furniture. Speaker state resets at page boundaries and large vertical gaps.
Possible cross-page speech stays unlinked and is reported instead of guessed.
This is not scene understanding, dialogue execution or perfect classification.

## Limits and safety

All limits must be positive. Form depth can only be lowered from 32 to stay
below backend nesting limits and avoid unbounded importer recursion:

| Limit | Default |
|---|---:|
| Input bytes | 32 MiB |
| PDF objects | 100,000 |
| Pages | 2,000 |
| Decoded page/Form stream bytes | 16 MiB each |
| Expanded decoded content bytes | 64 MiB total |
| Expanded page/Form operations | 1,000,000 |
| Form depth | 32 |
| Glyph callbacks | 1,000,000 |
| Decoded glyph text bytes | 16 MiB |
| Lines / blocks / doubts / warnings | 100,000 each |

Preflight follows invoked Form XObjects, charging repeated invocations to the
content/operation budget and rejecting cycles. Embedded fonts, backend parsing,
page-tree allocation and decompression can allocate before a limit is checked.
Limits are **not a hard process memory/CPU cap or hostile-PDF sandbox**. The
glyph limit bounds retained extraction, but callbacks after it may still be
processed by the backend before the import returns an error. Isolate processing
of untrusted PDFs when hard memory/time guarantees are required.

## Offline examples and tests

From this package directory, after fetching dependencies:

```text
cargo run --offline --locked --example import -- samples/synthetic.pdf
cargo run --offline --locked --example make_sample -- target/synthetic.pdf
cargo test --offline --locked
cargo fmt --all -- --check
cargo clippy --offline --locked --all-targets -- -D warnings
```

`import` writes JSON to stdout only after import succeeds. `make_sample` refuses
to overwrite any existing path. The included sample and all test PDFs are
synthetic. Its Type3 font draws simple boxes and maps them to our invented
English/Russian text; no copied font file, real screenplay or artwork is
included. The builder in `examples/support` reproduces it deterministically.

## License

MIT OR Apache-2.0. See the repository's root license files.
