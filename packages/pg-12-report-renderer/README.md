# Report renderer
<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

`report-renderer` converts a bounded neutral report JSON v1 document into deterministic CommonMark Markdown or a complete Typst document. An optional process wrapper compiles the Typst source to PDF. The crate has no network access or runtime dependency other than Typst for PDF compilation.

## API

```rust
use report_renderer::{from_json, render_markdown, RenderOptions};
let json_text = r#"{"version":1,"title":"Example","sections":[{"id":"one","title":"One","blocks":[]}]}"#;
let report = from_json(json_text).unwrap();
let rendered = render_markdown(&report, &RenderOptions::default());
println!("{}", rendered.text);
```

`render_typst` returns the same `Rendered { text, warnings }` shape. `compile_pdf(source, output_dir, typst_path)` writes `report.typ`, invokes `typst compile --root output_dir`, and returns `report.pdf`. Missing executables and nonzero exits are typed `Error` values; stderr is retained up to 8 KiB. Images are checked only for the Typst path. `PageSize::A4` is the default; use `PageSize::Letter` for US Letter.

## JSON v1

The complete Draft 2020-12 schema is [`schema.json`](schema.json). Unknown fields are rejected. Strings are plain text: inline Markdown or Typst is never interpreted.

### Report

| Field | JSON type | Required | Meaning |
|---|---|---:|---|
| `version` | integer `1` | yes | Schema version. |
| `title` | string | yes | Report title; must not be blank. |
| `subtitle` | string or null | no | Subtitle. |
| `author` | string or null | no | Author/credit. |
| `date` | string or null | no | ISO calendar date `YYYY-MM-DD`; never generated. |
| `language` | string or null | no | BCP-47 language tag, validated syntactically. |
| `summary` | string or null | no | Plain-text summary paragraph. |
| `sections` | array | yes | Top-level sections. |

### Section

| Field | JSON type | Required | Meaning |
|---|---|---:|---|
| `id` | string | yes | Unique stable ID across the entire tree. |
| `title` | string | yes | Section heading. |
| `level` | integer 1–4 | no (1) | Absolute output heading level. |
| `optional` | boolean | no (false) | Requires `include_optional` or explicit `include`. |
| `enabled` | boolean | no (true) | Disabled sections and all descendants are omitted. |
| `blocks` | array | yes | Ordered block list. |
| `sections` | array | no (`[]`) | Nested sections. |

### Blocks

Every block has a required string `type` discriminator. The following table documents all other fields; `?` means optional and permits null.

| Type | Fields and JSON types | Meaning |
|---|---|---|
| `paragraph` | `text: string` | Literal paragraph text. |
| `heading` | `text: string`, `level: integer 1–4` | Internal heading, excluded from the section TOC. |
| `bullets` | `items: string[]` | Unordered items; empty lists produce no output. |
| `numbered` | `items: string[]` | Ordered items starting at 1. |
| `table` | `columns: string[]`, `rows: string[][]`, `caption?: string` | At least one column; every row has exactly the column count. |
| `key_values` | `pairs: [string, string][]` | Key/value rows with generated Key and Value headers. |
| `code` | `text: string`, `language?: string` | Literal code; language is a nonempty ASCII alphanumeric token also permitting `_`, `+`, `-`. |
| `quote` | `text: string`, `attribution?: string` | Quotation and optional credit. |
| `callout` | `kind: string`, `text: string` | Kind is `note`, `warning`, `error`, or `success`. |
| `page_break` | none | PDF page break; Markdown HTML comment. |
| `image` | `path: string`, `caption?: string`, `width_percent?: number` | Referenced image; width >0 and ≤100, default 100 in Typst; ignored by Markdown. |

Image paths use portable relative syntax: no absolute path, URL/scheme, backslash, query, fragment, control character, empty component, `.` or `..` component. Typst checks for an existing regular file contained within `image_root` (including symlink resolution), or emits a warning and placeholder box; Markdown emits the reference without reading it. File content/format errors remain compiler errors.

## Selection

`RenderOptions` has `include: Option<Vec<String>>`, `exclude: Vec<String>`, `include_optional` (false), `toc` (false), `page`, and `image_root`. A section renders when it is enabled, not excluded, and either non-optional, explicitly included, or `include_optional` is true. Explicit inclusion does not override `enabled`; exclusion always wins. An omitted parent suppresses every nested section. Unknown IDs produce warnings (once per option, in input order).

## Outputs and escaping

Markdown uses LF line endings, `#` headings, GFM pipe tables, blockquote callouts, HTML page-break comments, and image links. Markdown punctuation is escaped in prose, table cells, headings and captions; image destinations are percent-encoded. Literal line breaks in inline text become generated `<br>` tags; leading spaces/tabs are entities to prevent accidental code blocks. Code blocks normalize LF and use a fence longer than any backtick run in their content. Typst uses a title block, optional outline, numbered headings, `table`, `block`, `raw`, and `quote`. All text and paths are encoded as Typst strings, so special characters, backticks, backslashes, and `//` remain data. Typst callouts are colored blocks. The source output is deterministic; omitted dates do not become today's date. PDF bytes depend on the installed compiler and fonts.

PDF compilation requires Typst on `PATH`, or pass `--typst /path/to/typst`. Verified with 0.14.2 and 0.15.1. Exactly one test uses a real compiler and skips with a message when absent; other tests use fake local executables. `compile_pdf` accepts `Option<&Path>` for the compiler argument; `None` uses PATH lookup by the OS.

## CLI

```text
report-renderer <in.json> (--md out.md | --typ out.typ | --pdf out.pdf) [--typst /path] [--include a,b] [--exclude c] [--include-optional] [--toc] [--page a4|letter]
```

Warnings go to stderr. A malformed input, invalid option, missing compiler, or compiler failure exits 1. PDF mode stages only selected, safe image files below the input directory and removes its temporary directory afterward. `--typ` resolves images relative to the output document's directory; arrange images there before rendering. Library `render_typst` checks against `image_root` (current directory by default) and retains relative paths; copy those assets to `compile_pdf`'s output directory before compiling. Neither the library compiler wrapper nor `--typ` copies images.

## Limits

Input is at most 16 MiB; the tree has at most 10,000 sections, depth 8 (top level is depth 1), and 100,000 blocks in aggregate. Disabled and omitted sections count toward limits. Validation happens before rendering and uses no panics. `read_report` bounds streams as well as strings. `validate` checks programmatic models; invalid models passed directly to either renderer return empty text and `Warning::InvalidReport`. JSON Schema describes local constraints; runtime additionally checks global ID uniqueness, rectangular tables, aggregate limits, calendar dates, and BCP-47 syntax.

## Decisions

Nested `level` values are absolute, which keeps JSON stable when a subtree moves. `include` is additive, following the stated selection formula; it is not a whitelist of ordinary sections. Included descendants cannot resurrect an omitted parent. The Markdown TOC uses deterministic numbered anchors rather than deriving IDs from punctuation or language; it is flat to support arbitrary absolute heading levels. Typst's outline includes selected section headings only. The Typst metadata date is `none` when absent. Dates use the unambiguous ISO date-only form, with years 0001–9999. BCP-47 tags are checked syntactically without registry lookup; Typst receives the primary language subtag because its `lang` option is not a complete BCP-47 tag. Private-use tags leave Typst's default language. The Typst renderer does not decode image bytes; it only verifies containment and file existence. Warning order follows selection option order, then rendered image order.

## Not supported

There is no inline markup, Markdown-to-Typst conversion, font bundling, image embedding in Markdown, URL image fetching, or date generation. The compiler process is never a shell command and never accesses the network.

From this package directory, run `cargo run --locked -- samples/synthetic-report.json --md report.md`, or replace `--md report.md` with `--pdf report.pdf`. Golden samples include every block, Cyrillic, nested sections and optional content. Run offline checks with a worktree-local `CARGO_TARGET_DIR`: `cargo test --locked --offline`, `cargo fmt --check`, and `cargo clippy --locked --offline --all-targets -- -D warnings`.

The package defines its own JSON shape and does not depend on another playground package. Reference specifications used: [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12), [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/), [GFM tables](https://github.github.com/gfm/#tables-extension-), [RFC 5646 language tags](https://www.rfc-editor.org/rfc/rfc5646), and [Typst reference](https://typst.app/docs/reference/). MIT OR Apache-2.0; both license texts are included.
