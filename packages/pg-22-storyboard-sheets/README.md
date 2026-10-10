<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# storyboard-sheets

`storyboard-sheets` turns a bounded neutral shot-list JSON document and local
frame images into a deterministic Typst document. The document prints as a
contact sheet with fixed 2×3 or 3×4 cells, captions, shot IDs, page numbers,
and a placeholder for every absent or unsafe frame. PDF output is an optional
step that invokes an installed Typst executable.

```rust
use storyboard_sheets::{from_json, render_typst, Options};
let board = from_json(br#"{"version":1,"shots":[{"id":"A"}]}"#)?;
let rendered = render_typst(&board, &Options::default())?;
assert!(rendered.source.contains("no frame"));
# Ok::<(), storyboard_sheets::Error>(())
```

## JSON v1

The shape is a local copy of the PG-16 neutral shot-list idea; this crate has
no dependency on PG-16 or any other package. Unknown fields are ignored while
the fields below are structurally checked. `schema.json` is a draft 2020-12
document; Rust additionally applies byte, path, uniqueness and nominal-rate
rules.

| Field | Meaning |
|---|---|
| `version` | Required integer `1`. |
| `title` | Optional project title string. |
| `frame_rate` | Optional `24`, `25`, `30`, or `{num,den}`; rational rates round to the nearest nominal label rate (half up), so `24000/1001` labels at 24. |
| `aspect` | Optional `16:9` (default), `4:3`, `2.39:1`, `1:1`, or `9:16`. |
| `shots` | Required ordered array, at most 10,000 items. |
| `shots[].id` | Required unique nonempty trimmed string. |
| `shots[].name` | Optional display name. |
| `shots[].duration_frames` | Optional positive frame count; used by timecodes. |
| `shots[].scene` | Optional scene heading or location. |
| `shots[].description` | Optional action text. |
| `shots[].dialogue` | Optional dialogue; printed in italics. |
| `shots[].camera` | Optional camera/shot direction such as `WS` or `pan left`. |
| `shots[].frame` | Optional object or `null`; absent means a placeholder. |
| `shots[].frame.path` | Required relative PNG, JPG or SVG path. `..`, `.`, absolute paths, URLs, backslashes, drive-colons, empty components and control characters are rejected. |
| `shots[].frame.alt` | Optional image alternative text. |
| `shots[].notes` | Optional additional caption text. |

## API and layout options

`render_typst(&Board, &Options)` returns `Rendered { source, warnings,
assets }`. `source` is a complete self-contained `.typ` file; `assets` is the
stable list of existing relative frame paths. `compile_pdf` writes a temporary
Typst source under `assets_root`, runs `typst compile --root assets_root`, and
returns captured stderr. The source uses only local files and can be reviewed
or compiled separately.

| Option | Values / default |
|---|---|
| `grid` | `TwoByThree` (default) or `ThreeByFour`; an empty board still has one page. |
| `page` | `A4` (default) or `Letter`. |
| `landscape` | `false` (portrait) by default; swaps paper dimensions when true. |
| `show_timecodes` | `false`; when true, duration labels are `HH:MM:SS:FF` and require both duration and rate. |
| `caption_fields` | Ordered list of `Name`, `Scene`, `Camera`, `Timecode`, `Description`, `Dialogue`, `Notes`; duplicate fields are rejected. |
| `max_caption_chars` | 100 by default, bounded to 1–1024; long text is truncated with `…`. |
| `title_page` | `false`; when true, adds a count page before the grid. |
| `start_page_number` | 1-based footer number (default 1). |
| `assets_root` | Directory used to resolve and contain frame paths (default `.`). |

Each cell has a fixed frame box and caption stack. A missing frame, an invalid
path, a path outside `assets_root`, or a non-file produces a dashed box with the
shot ID and `no frame`, plus a typed warning. User text is escaped into Typst
strings; it cannot become Typst markup. No image scaling or conversion is
performed; Typst's `contain` fit preserves the source image inside the box.

## CLI

```text
storyboard-sheets shots.json --typ board.typ [--assets frames] [--grid 2x3|3x4] [--page a4|letter] [--landscape] [--timecodes]
storyboard-sheets shots.json --pdf board.pdf [--assets frames] [--typst /path/to/typst]
```

Typst is optional at runtime for `.typ` output and required for PDF output. The
sample was verified with Typst 0.14.2. No fonts are bundled; the compiler's
font environment is used.

## Limits and decisions

Input is capped at 16 MiB and 10,000 shots. Text fields are capped at 16 KiB,
IDs at 256 UTF-8 bytes, image paths at 4096 bytes, and all output is
stable LF-terminated source. Timecodes are non-drop frame labels: rational
rates are rounded to 24/25/30 for labels, with frame remainder in `FF`; elapsed
wall-clock conversion and drop-frame notation are intentionally unsupported.
Page numbers count the optional title page and begin at `start_page_number`.

This package does not fetch URLs, dereference network assets, bundle fonts,
convert images, or produce PDFs without Typst. The JSON schema documents
structure; runtime validation remains authoritative for safety and limits.

## Offline verification

```text
export CARGO_TARGET_DIR=$PWD/target
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The integration tests cover grid/page arithmetic, malformed and oversized
input, duplicate IDs, every path rejection, Typst escaping, Unicode truncation,
timecode rates, options, warnings, determinism, CLI output, and an optional
sample PDF test that is skipped when `typst` is absent.

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
