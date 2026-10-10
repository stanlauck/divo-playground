# PG-18: Screenplay length estimator

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

Standalone Rust library and CLI that reads a Fountain screenplay and reports
the length of every scene in eighths of a page, the way production breakdowns
count them. It models a fixed Courier 12 pt layout on US Letter or A4, lays the
elements out line by line with the usual pagination rules, and converts the
lines each scene owns into eighths. The result is deterministic: the same text
and profile always give byte-identical JSON. No network, no fonts, no PDF.
MIT OR Apache-2.0.

```rust
use screenplay_length::{estimate, PageSize};

let text = "INT. KITCHEN - DAY\n\nA kettle whistles.\n\nMARA\nTea?\n";
let est = estimate(text, PageSize::Letter)?;
assert_eq!(est.scenes[0].eighths, 1);
assert_eq!(est.scenes[0].pages, "1/8");
assert_eq!(est.total.pages, 1);
# Ok::<(), screenplay_length::Error>(())
```

## API

| Item | Purpose |
|---|---|
| `estimate(text, PageSize) -> Result<Estimate>` | Estimate with the default profile of `PageSize::Letter` or `PageSize::A4` |
| `estimate_with(text, &LayoutProfile) -> Result<Estimate>` | Estimate with explicit numbers; the profile is validated first |
| `PageSize::profile()` / `PageSize::geometry()` | The default `LayoutProfile` and the physical constants it is derived from |
| `LayoutProfile::from_json(&str)` / `validate()` | Load a profile (missing fields take the Letter defaults) and range-check it |
| `parse(text) -> Result<Document>` | The Fountain parser alone (`Document { title_page, elements }`) |
| `paginate(&Document, &LayoutProfile) -> Estimate` | The layout pass alone |
| `wrap(text, width) -> Vec<String>` | The greedy word wrapper used for every element |
| `lines_to_eighths(lines, lines_per_page)` / `eighths_to_pages(eighths)` | The rounding and the `"1 2/8"` formatter |
| `to_json(&Estimate) -> String` / `to_table(&Estimate) -> String` | The two CLI renderings |
| `MAX_INPUT_BYTES`, `MAX_INPUT_LINES`, `SCHEMA_VERSION` | Limits and the output schema version |

Errors (`Error`): `InputTooLarge`, `TooManyLines`, `InvalidProfile`. Fountain
text itself never fails to parse: every line is some element.

## CLI

```text
screenplay-length <file.fountain | -> [--a4] [--json] [--profile profile.json]
```

Default output is a table; `--json` prints the JSON below; `--a4` selects the
A4 profile; `--profile` loads a JSON profile (any subset of the `layout`
fields, see `samples/profile.example.json`) and overrides `--a4`; `-` reads
standard input. Exit codes: 0 success, 1 error (message on stderr), 2 usage.

```text
page size: letter  (55 lines/page)
title page: yes (page 1)
  #  no.    pages  eighths  lines  start-end  heading
  1  -      1/8          1      2     2-2     (before first heading)
  2  1      4/8          4     25     2-2     EXT. HARBOUR WALL - DUSK
  3  2      1 4/8       12     83     2-3     INT. CUSTOMS HOUSE - RECORDS ROOM - NIGHT
  4  3      3/8          3     23     4-4     CELLAR UNDER THE FOURTH LANTERN - DAWN
total: 4 page(s), 20 eighths (2 4/8), 133 body lines
```

## Output JSON

```json
{"version":1, "page_size":"letter", "layout":{...}, "title_page":true,
 "scenes":[{"index":0,"number":null,"heading":null,"start_page":2,"end_page":2,
            "line_count":2,"eighths":1,"pages":"1/8"}],
 "total":{"pages":4,"eighths":20,"lines":133}}
```

| Field | Meaning |
|---|---|
| `version` | Schema version, always `1` |
| `page_size` | Label of the profile: `letter`, `a4`, or the `page_size` of a custom profile |
| `layout` | Every number the estimate used (the full `LayoutProfile`, fields below) |
| `title_page` | `true` when a title page was found; it then occupies page 1 |
| `scenes[].index` | Zero-based scene position |
| `scenes[].number` | Scene number from a trailing `#12A#` in the heading, else `null` |
| `scenes[].heading` | Heading text without the forcing `.` and without the number; `null` for the pseudo-scene of material before the first heading |
| `scenes[].start_page` | Page of the heading (1-based, counting the title page) |
| `scenes[].end_page` | Page of the last line the scene owns |
| `scenes[].line_count` | Lines owned: heading through the line before the next heading, including blank spacing and page-break padding |
| `scenes[].eighths` | `round_half_up(line_count * 8 / lines_per_page)`, minimum 1 when `line_count > 0` |
| `scenes[].pages` | `eighths` as `"3/8"`, `"1"`, `"1 2/8"` |
| `total.pages` | `ceil(body_lines / lines_per_page)` plus 1 for a title page |
| `total.eighths` | Sum of the scene eighths (so it may differ from `total.pages * 8` by rounding) |
| `total.lines` | Body lines used; equals the sum of `line_count` |

Field order is fixed, output ends with one LF, no timestamps.

## Layout rules

Courier 12 pt is a monospaced font with 10 characters per inch horizontally
and 6 lines per inch vertically. Both page sizes use the same margins in
inches, so element widths are identical; only the number of lines per page
differs. The `LayoutProfile` holds the integers actually used; the physical
numbers are how they were derived.

| Constant | US Letter | A4 |
|---|---|---|
| Page | 8.5 x 11 in | 210 x 297 mm = 8.27 x 11.69 in |
| Margins top / bottom | 1 in / 1 in | 1 in / 1 in |
| Margins left / right | 1.5 in / 1 in | 1.5 in / 1 in |
| Usable height | 9 in = 54 lines | 9.69 in = 58.1 lines |
| `lines_per_page` | **55** | **58** |
| Usable width | 6 in = 60 chars | 5.77 in, rounded up to the same 60 chars |

On Letter the strict arithmetic gives 54 lines; production tools vary between
54 and 57 because of header and footer allowances, so 55 is used as the middle
value. Override it with a profile if a tool you compare against uses another.
On A4 the slightly narrower page is ignored for widths (dialogue and action
columns sit well inside the margins).

| Element | Profile field | Width (chars) | Derivation |
|---|---|---|---|
| Action, lyrics | `action_width` | 60 | 1.5 in to 7.5 in from the left edge |
| Scene heading | `scene_heading_width` | 60 | same as action |
| Character cue | `character_width` | 33 | starts 3.7 in from the left edge, ends at the right margin |
| Parenthetical | `parenthetical_width` | 25 | starts 3.1 in |
| Dialogue | `dialogue_width` | 35 | starts 2.5 in, 3.5 in wide |
| Dual dialogue column | `dual_dialogue_width` | 28 | two columns of equal width inside the 60-char area with a gutter; cue, parenthetical and text all wrap at 28 |
| Transition | `transition_width` | 60 | right-aligned, but alignment does not change the line count |
| Centered text | `centered_width` | 60 | centered in the action area |

Vertical rules:

* Every element starts with exactly one blank line, except at the top of a
  page and except inside a dialogue block (cue, parentheticals and dialogue
  are contiguous). Successive action lines are each one line; a Fountain
  paragraph of several lines keeps each line.
* Wrapping is greedy on single spaces; runs of spaces inside a line collapse;
  leading indentation (tabs = 4 spaces) stays on the first line. A word longer
  than the width is hard-split at the width. Each Unicode scalar value counts
  as one column.
* Orphan headings: a scene heading needs `heading_keep_lines` (2) usable lines
  after it on the same page; otherwise it moves to the next page. The scene
  that precedes it owns the lines left empty.
* A page break `===` starts a new page; the padding belongs to the scene the
  break sits in. A break at the top of a page does nothing.
* A character cue never ends a page. A dialogue block that does not fit keeps
  the cue plus at least `dialogue_split_min_lines` (2) body lines and a
  `(MORE)` line on the current page, then continues on the next page with a
  new `NAME (CONT'D)` cue (not added again when the cue already ends in
  `(CONT'D)`). When fewer than 2 lines would fit, the whole block moves.
  Splitting repeats for very long speeches.
* Dual dialogue (a block followed by a block whose cue ends in `^`) is one
  unit as high as the taller column; it is kept together on one page unless
  it is taller than a whole page, in which case it spills.
* The title page is one page of its own (page 1) and is excluded from all
  scene lengths; `total.pages` includes it.
* Material before the first scene heading (`FADE IN:`, cold-open action) forms
  a pseudo-scene with `heading: null`.
* Eighths: `round_half_up(line_count * 8 / lines_per_page)`, computed in
  integers; a non-empty scene is at least one eighth. On Letter one eighth is
  6.875 lines: 1 to 10 lines give 1/8, 11 to 17 lines give 2/8, 55 lines give
  8/8.

## Fountain subset

The parser follows the public Fountain syntax reference
(<https://fountain.io/syntax>). It implements:

| Element | Rule used |
|---|---|
| Title page | First non-empty block starts with a known `Key:` (title, credit, author(s), source, draft date, date, contact, copyright, notes, revision, format); later keys in the block may be anything; values continue on lines indented by 3+ spaces or a tab; ends at the first blank line |
| Scene heading | Line starting with `INT`, `EXT`, `EST`, `INT./EXT`, `INT/EXT`, `I/E` followed by `.` or a space (case-insensitive), or forced with `.` + alphanumeric; a trailing `#…#` is the scene number |
| Action | Anything else; `!` forces it; indentation kept, tabs = 4 spaces |
| Character | Paragraph of 2+ lines whose first line is uppercase outside parentheses (extensions like `(V.O.)` may be lowercase), or forced with `@`; a trailing `^` marks the right column of dual dialogue |
| Parenthetical | Line inside a dialogue block wrapped in `(` `)` |
| Dialogue | Every other line of the block; a line of two or more spaces is an intentional blank line inside the block |
| Lyrics | `~` lines, in or out of dialogue |
| Transition | Single uppercase line ending in `TO:`, or any line forced with `>` (unless it also ends in `<`) |
| Centered | Paragraph whose first line is `>text<` |
| Page break | Line of three or more `=` and nothing else |
| Section, synopsis | Lines starting with `#` or `=`: removed, take no space |
| Note, boneyard | `[[…]]` and `/*…*/`, possibly spanning lines: removed; a line that becomes empty disappears entirely, so it does not even split a paragraph |
| Line endings | LF, CRLF and a leading BOM are accepted |

## Limits

Input up to 16 MiB and 1,000,000 lines (`MAX_INPUT_BYTES`,
`MAX_INPUT_LINES`); larger input returns an error, the CLI also refuses
non-UTF-8. Profile numbers are range-checked (`lines_per_page` 8..=1000,
widths 1..=1000, `heading_keep_lines <= lines_per_page - 2`,
`dialogue_split_min_lines` 1..=`lines_per_page - 3`). Counters saturate; no
input panics.

## Decisions

* Lines per page: 55 for Letter, 58 for A4 (see Layout rules). The profile
  is the escape hatch; the numbers are echoed in the output so any consumer
  can see what was used.
* Scene length includes the blank line and page padding after the scene, up
  to the next heading, because that is what a page count on paper includes.
  The sum of scene lines therefore equals the body lines of the script.
* The title page is counted as a whole page regardless of its content.
* A single uppercase line followed by a blank line is action, not a cue (the
  spec requires a following line); a cue needs at least one body line.
* Element detection is per paragraph: a forced heading, forced transition or
  natural heading on the first line makes the remaining lines of that
  paragraph action.
* `>` lines are transitions even when lowercase, as the spec says; a line that
  starts with `>` and ends with `<` is centered text.
* Dual dialogue columns wrap at 28 characters and the cue sits inside the
  column; `(MORE)`/`(CONT'D)` splitting is not applied to dual dialogue.
* `(CONT'D)` cues in the source are kept as written and not merged with the
  previous block.
* `total.eighths` is the sum of rounded scene values, not `pages * 8`.
* Headings wider than 60 characters wrap; the orphan rule uses the wrapped
  height.

## Not supported

* Proportional fonts, other point sizes, or real text metrics: a column is
  one scalar value, so CJK double-width glyphs, combining marks, emoji and
  tabs inside text are not modelled.
* Fountain emphasis (`*`, `_`), inline HTML, `\` escapes: the markers are
  counted as ordinary characters.
* Title-page values do not influence the page count (always one page).
* Scene numbers are not validated or renumbered.
* No PDF or FDX output, no comparison with any particular screenwriting
  application's pagination; results are estimates.

## Development

```sh
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run -- samples/synthetic.fountain --json
```

`samples/synthetic.fountain` is a wholly invented script that exercises every
element; `samples/synthetic.letter.json` and `samples/synthetic.a4.json` are
its golden outputs. To regenerate them after an intentional layout change run
the CLI with `--json` (and `--a4 --json`) and review the diff.
