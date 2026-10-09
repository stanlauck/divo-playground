# PG-05: production breakdown export

Independent Rust crate/CLI: strict neutral JSON v1 → **numbered, tagged FDX**
and **CSV of the supplied shooting order**. No host application dependency,
networking, script execution, automatic scheduling or layout/pagination engine.
All checked-in data is invented. License: MIT OR Apache-2.0.

## Quick start

```rust
use pg_05_breakdown_export::{from_json, to_csv, to_fdx};
let input = br#"{"version":1,"elements":[],"scenes":[]}"#;
let breakdown = from_json(input.as_slice())?;
let fdx = to_fdx(&breakdown)?;
let schedule = to_csv(&breakdown)?;
# Ok::<(), pg_05_breakdown_export::Error>(())
```

```sh
cargo run --locked --offline -- fdx samples/synthetic.json output.fdx
cargo run --locked --offline -- csv samples/synthetic.json schedule.csv
# stdin/stdout are "-", existing output files are never overwritten.
cargo run --locked --offline -- csv samples/synthetic.json - --raw-csv
```

Run inside this package, after dependencies are installed. `from_json` accepts
any Rust `Read`, bounds the bytes read and tolerates a UTF-8 BOM. `to_json`
serializes compact neutral JSON and omits empty/default fields, so pretty-print
or materialized defaults cannot inflate an accepted document beyond its input
byte/value budgets. Both exporters also validate
manually constructed `Breakdown` values. Failures contain stable codes and
structural paths, never source text, user keys/IDs or filesystem names.
CLI output is serialized before opening the destination. I/O failure can leave
a partial newly created file or stdout stream; writing is not transactional.

## JSON v1 contract

`schema.json` is a Draft 2020-12 structural schema. The Rust validator is
authoritative for reference integrity, UTF-8 byte budgets and XML characters.
Unknown fields, duplicate JSON keys at any depth, trailing data, invalid UTF-8,
null required fields and numeric coercions are rejected. Optional scalar fields
can be omitted or null. Strings are preserved without Unicode normalization.

- Root: `version:1`, required `elements`/`scenes` arrays, optional `title`,
  optional `shooting_days` (defaults to empty).
- Element: globally unique `id`, `category`, `name`. Distinct elements cannot
  share the exact `(category,name)` pair, because importers can merge labels.
  Categories: `cast`, `background_actors`, `stunts`, `vehicles`, `props`,
  `camera`, `special_effects`, `wardrobe`, `makeup_hair`, `animals`,
  `animal_wrangler`, `music`, `sound`, `art_department`, `set_dressing`,
  `greenery`, `special_equipment`, `security`, `additional_labor`,
  `visual_effects`, `mechanical_effects`, `miscellaneous`.
- Scene: globally unique `id`, unique nonblank `number` (e.g. `"10A"`),
  `int_ext` = `interior | exterior | interior_exterior`, nonblank single-line
  `set`, `time_of_day` = `day | night | dawn | dusk | morning | afternoon |
  evening | continuous | later`, integer `pages_eighths` in 0…80,000.
  `synopsis`/`notes` default to `""`; optional nonblank single-line
  `script_day`/`unit`. `elements` defaults to empty and contains
  `{element_id,quantity?}`. Quantity defaults to 1 and is 1…1,000,000;
  the same element cannot occur twice within one scene.
- Shooting day: globally unique `id`, nonblank single-line `label`, optional
  calendar `date` (`YYYY-MM-DD`, years 0001…9999), required `scenes` array of
  scene IDs in desired shooting order. A scene is assigned at most once across
  all days. Empty days are valid and create no CSV scene row.
- IDs: 1…128 ASCII bytes, first alphanumeric, then letters/digits/`_.:-`.
  Global means shared across element/scene/shooting-day IDs.

`scenes` is narrative order, not automatically sorted by scene number.
CSV visits shooting days in their given array order, then each day's scene IDs
in given order, then unassigned scenes in narrative order. Dates do not reorder
anything. If no days are supplied, CSV uses narrative order. FDX always uses
narrative order because it is a Script import carrier, **not a shooting plan**.

## FDX and Movie Magic Scheduling

The MMS manual documents importing `.fdx` using **File → Import Script**.
Final Draft also documents exporting tagged scripts and synopsis tags to MMS:

- [MMS import guide](https://mms-docs.ep.com/Breakdown/ImportingBreakdownSheets.html)
- [Final Draft tagged export](https://kb.finaldraft.com/hc/en-us/articles/15574927910036-How-do-I-get-my-tagged-FD-script-into-Movie-Magic-Scheduling)
- [Final Draft synopsis tags](https://kb.finaldraft.com/hc/en-us/articles/30533150147604-How-do-I-use-Tags-to-add-a-Synopsis-to-a-scene)

**Supported import route:** only the **FDX-tag-aware** Script import described
in the current EP manual, which explicitly says tagged elements are imported.
Legacy/scene-only FDX paths that require a `.sex` scheduling export for element
categories are **not supported**. Do not use this writer for those paths; there
is no automatic fallback or silent claim of compatibility with every MMS
release. A numbered FDX scene alone is not evidence that its tags were imported.
No specific installed MMS version has been verified here.

This writer emits a `FinalDraft DocumentType="Script" Version="1"` document,
scene-heading `Paragraph/@Number`, `SceneProperties` (number/length/start page),
and `Summary/Paragraph/Text`. Each supplied synopsis, location, script day,
unit, note and referenced element gets a tagged stand-in Action paragraph.
These paragraphs are breakdown data, **not fabricated screenplay prose**.
An element's quantity is visible untagged text; no native MMS quantity mapping
is claimed. Dates/day assignments remain CSV-only.

Tag wiring is `Text/@TagNumber → Tag/@Number → DefId → TagDefinition/@Id`,
with `CatId → TagCategory/@Id`. Standard category UUIDs and this structure were
independently implemented from format observations in the open-source Beat
exporter, pinned to commit
`7778ad043ae3de152bdb43daba0fbbdf3481c787`:
[BeatFDXExport.m](https://github.com/lmparppei/Beat/blob/7778ad043ae3de152bdb43daba0fbbdf3481c787/Frameworks/BeatFileExport/BeatFileExport/Export/Modules/Final%20Draft/BeatFDXExport.m).
No third-party code, templates or screenplay fixtures are redistributed.
The `SceneProperties/Summary` observation is also documented by
[open-fdx-toolkit](https://github.com/sfingali/open-fdx-toolkit/blob/243486e6573e52d5e768d0c91bf366eb93324613/FDX_SPEC.md).
Generated definition IDs use deterministic UUIDv5 under a fixed package
namespace, keyed by neutral element ID or scene ID/field. Shared Location,
Script Day and Unit definitions are deduplicated by `(category,label)` and
keyed by that category UUID/label, not the first scene that mentions them.
IDs are not random or based on input array index.

**Validation boundary:** XML structure, tag/category references, escaping and
metadata are tested independently. No proprietary MMS/Final Draft installation
or authoritative FDX XSD is bundled. A live application import has **not** been
certified. MMS versions may recalculate scene length/page from the carrier's
stand-in paragraphs. `Length="13/8"` records supplied eighths, and `Page` is
`floor(sum(previous pages_eighths)/8)+1`, not rendered pagination. Use neutral
JSON/CSV as the authoritative page/quantity/assignment data, and verify target
MMS behavior before relying on an imported plan. No native `.mms`/`.sex` file,
FDX reader, fidelity to a real screenplay, or roundtrip claim.

## CSV

UTF-8 without BOM, comma delimiter, every cell quoted, `"` doubled, CRLF record
terminators. Embedded line endings are preserved. Fixed columns:

```text
order,shooting_day_id,shooting_day,date,scene_id,scene_number,int_ext,set,time_of_day,pages_eighths,pages,synopsis,script_day,unit,elements_json,notes
```

`pages_eighths` is authoritative integer data. `pages` is exact display text:
`0`, `7/8`, `1`, `1 5/8`. `elements_json` is a JSON array with
`id/category/name/quantity`, preserving punctuation and avoiding delimiter-based
list ambiguity.

`to_csv` defaults to `CsvMode::SpreadsheetSafe`: if a cell starts with
`=`, `+`, `-` or `@` after whitespace/control prefixes, prepend `'` before the
entire original cell. Quoting is not formula protection. This mitigation can
change displayed text and is not a guarantee for every spreadsheet or later
editing/export operation. `to_csv_with_mode(..., CsvMode::Raw)` / `--raw-csv`
preserves source cells for programmatic consumers; do not open untrusted raw CSV
in a spreadsheet.

## Budgets

- JSON input ≤8 MiB, JSON depth ≤16, values ≤500,000; duplicate keys rejected
  before typed deserialization.
- At most 10,000 each elements/scenes/shooting days; 200,000 total scene-element
  references. At most 10,000 assignments per day, no repeats globally.
- IDs ≤128 bytes; scene numbers ≤32; single-line labels/title ≤1,024;
  synopsis/notes ≤65,536. Total validated text ≤8 MiB. XML 1.0 characters only;
  tabs/CR/LF and Unicode U+2028/U+2029 line/paragraph separators are rejected in
  single-line fields and allowed only in multiline synopsis/notes.
- Each output ≤64 MiB, including expansion when one element is referenced many
  times. Both outputs are in-memory; no claim of constant-memory streaming.
- Schema string limits count Unicode scalar characters; runtime limits count
  UTF-8 bytes and can be stricter. Rust integer fields do not coerce decimal
  tokens, strings or floats. Enable schema format checks for dates; global
  reference/label/assignment invariants still require Rust.

## Offline checks

```sh
cargo test --locked --offline
cargo fmt --check
cargo clippy --locked --offline --all-targets -- -D warnings
python tests/check_outputs.py
python examples/make_schema.py --write
```

Rust tests independently parse emitted XML (`roxmltree`) and CSV, validate
tag wiring, exact eighths/order, Unicode/line endings, formula mitigation,
malformed inputs, budgets, deterministic IDs and non-overwriting CLI behavior.
The Python stdlib checker reads only synthetic output fixtures. Schema and
output fixtures are reproducible; tests never fetch network resources.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
