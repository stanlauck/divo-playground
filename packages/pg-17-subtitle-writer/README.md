# PG-17: Subtitle writer

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

Standalone Rust library and CLI that turns a neutral dialogue timing JSON (v1)
into **SRT**, **WebVTT** or **TTML** with speaker labels, and runs readability
checks (reading speed, line length, lines per cue, duration) and structural
checks (ordering, overlap, unique ids, empty lines). Violations are reported
as a JSON array; the CLI `--strict` flag turns any violation into exit status 1.

Writer only: there are **no subtitle parsers**. No network, no timestamps,
byte-identical output for identical input. MIT OR Apache-2.0.

```rust
use subtitle_writer::{check, parse, write, Checks, Format};

let document = parse(r#"{
  "version": 1,
  "cues": [
    {"id": "c1", "speaker": "ANNA", "lines": ["Hello."], "start_ms": 0, "end_ms": 1500}
  ]
}"#)?;
let srt = write(&document, Format::Srt)?;
assert_eq!(srt, "1\n00:00:00,000 --> 00:00:01,500\nANNA: Hello.\n");
let violations = check(&document, &Checks::default())?;
assert!(violations.is_empty());
# Ok::<(), subtitle_writer::Error>(())
```

Public API: `parse`, `read_input`, `Document`, `Cue`, `write` /
`write_srt` / `write_vtt` / `write_ttml`, `Format`, `format_timecode`,
`escape_text`, `check`, `Checks`, `Violation`, `Rule`, `Metric`,
`reading_speed`, `report_json`, `Error`, and the `MAX_*` / `DEFAULT_*`
constants.

## Input: neutral dialogue timing JSON v1

See `schema.json` (JSON Schema draft 2020-12) and the invented
`samples/synthetic.json`. The shape is this package's own; it does not depend
on any other playground package.

| Field | Type | Required | Meaning |
|---|---|---|---|
| `version` | integer | yes | Must be `1`. |
| `title` | string or `null` | no | Document title; at most 1024 characters. Only TTML carries it (`ttm:title`). |
| `language` | string or `null` | no | BCP-47-style tag, ASCII letters/digits/hyphens, at most 64 characters. Only TTML carries it (`xml:lang`); default `und`. |
| `cues` | array | yes | Cues in presentation order; at most 100 000. May be empty. |
| `cues[].id` | string | yes | Non-empty identifier, at most 256 characters. Used as the WebVTT cue identifier and (when all ids are unique ASCII NCNames) as TTML `xml:id`. Uniqueness is a check, not a parse error. |
| `cues[].speaker` | string or `null` | no | Speaker label, 1–256 characters, not whitespace-only. `null` and absence are equivalent. |
| `cues[].lines` | array of strings | yes | 1–16 lines as displayed, each at most 1024 characters. A line must not contain a line break. |
| `cues[].start_ms` | integer | yes | Start, wall-clock milliseconds, `0 ≤ value ≤ 359 999 999` (`99:59:59.999`). |
| `cues[].end_ms` | integer | yes | Exclusive end, same range. |

All text fields (`title`, `id`, `speaker`, `lines[]`) reject C0/C1 control
characters (so tabs and newlines), U+2028, U+2029, U+FFFE, U+FFFF and the
substring `-->` (a timing marker in SRT/WebVTT). Lengths count Unicode scalar
values, not bytes. Unknown fields are ignored. Times must be non-negative
integers; floats and strings are rejected. A leading UTF-8 BOM is skipped.

## Checks

All limits are inclusive: a value equal to the limit passes. Characters are
Unicode scalar values (`str::chars`), excluding line breaks and excluding the
speaker label.

| Rule (`rule`) | Reported when | `value` | `limit` | Default |
|---|---|---|---|---|
| `duplicate_id` | the id was used by an earlier cue (reported on each repeat) | number of cues with this id so far | `1` | — |
| `empty_line` | a line is empty or whitespace-only (one violation per line) | `0` | `1` | — |
| `end_not_after_start` | `end_ms ≤ start_ms` | duration in ms (may be ≤ 0) | `1` | — |
| `not_sorted` | the cue starts before the previous cue's `start_ms` | this cue's `start_ms` | previous cue's `start_ms` | — |
| `overlap` | the cue starts before the previous cue's `end_ms` (and is not `not_sorted`) | overlap in ms | `0` | — |
| `too_short` | duration < `min_duration_ms` (only for positive durations) | duration in ms | `min_duration_ms` | 1000 ms |
| `too_long` | duration > `max_duration_ms` | duration in ms | `max_duration_ms` | 7000 ms |
| `too_many_lines` | more lines than `max_lines` | line count | `max_lines` | 2 |
| `line_too_long` | a line has more characters than `max_line_length` (one violation per line) | characters | `max_line_length` | 42 |
| `reading_speed` | characters ÷ seconds > `max_cps` (only for positive durations) | CPS rounded to 2 decimals (float) | `max_cps` | 17 |

The CPS comparison itself is exact integer arithmetic
(`chars × 1000 > max_cps × duration_ms`); only the reported value is rounded.
Touching cues (`end_ms` of one equals `start_ms` of the next) are not overlaps.
"Previous cue" always means the previous element of the array.

Configuration (`Checks`) must have every limit `> 0` and
`max_duration_ms ≥ min_duration_ms`; otherwise `check` returns `Error::Config`.

### Report

`report_json` (and the CLI) emit a pretty-printed JSON array, 2-space indent,
LF, trailing newline; `[]` when clean. Violations are ordered by cue (input
order), then by rule in the table order above, then by line index. Messages
are deterministic and never quote input text.

```json
[
  {
    "cue": "c6",
    "rule": "too_short",
    "message": "duration 800 ms is below the minimum of 1000 ms",
    "value": 800,
    "limit": 1000
  }
]
```

## Output formats

All writers: LF line endings, UTF-8 without BOM, trailing newline, cues in
input order, timecodes with two-digit hours. Writers first validate the
document and refuse (as `Error::Invalid`) a cue whose `end_ms ≤ start_ms` or
that has an empty/whitespace-only line, because no format can carry those;
run `check` first to see them as violations.

### SRT (`--format srt`)

Sequential numbering from 1, `HH:MM:SS,mmm`, the speaker as a `NAME: `
prefix on the first line, no escaping, blocks separated by one empty line.
An empty document produces an empty string.

```text
1
00:00:01,000 --> 00:00:02,800
MIRA: Did you hear that?

2
00:00:03,000 --> 00:00:06,500
JONAS: Only the wind.
It always sounds like footsteps up here.
```

### WebVTT (`--format vtt`)

Starts with `WEBVTT`. Each cue block: cue identifier (= `id`), timing line
`HH:MM:SS.mmm`, payload. The speaker becomes a `<v NAME>` voice span opened at
the start of the first line; the closing `</v>` is omitted (it is optional at
the end of the cue). `&`, `<`, `>` are escaped in both payload and speaker
name. An empty document produces `WEBVTT\n`.

```text
WEBVTT

c4
00:00:09.200 --> 00:00:11.500
<v MIRA>Wind doesn't knock twice &amp; wait.

c6
00:00:15.200 --> 00:00:16.000
<v JONAS>Pressure is &lt;dropping&gt; fast, see the gauge?
We should go.
```

### TTML (`--format ttml`)

TTML 1.0 with `ttp:timeBase="media"` and clock-time `HH:MM:SS.mmm` values, in
the IMSC style of one `<p>` per cue inside one `<div>`. Distinct speakers
become `ttm:agent` elements (`agent1`, `agent2`, … in first-appearance order)
in `<head><metadata>`, referenced by `ttm:agent` on each `<p>`. Lines are
joined with `<br/>`. `title` becomes `ttm:title`; `language` becomes `xml:lang`
(default `und`). `&`, `<`, `>` are escaped in text and `"` additionally in
attributes. `xml:id` of each `<p>` is the cue id when every id is unique and
matches `[A-Za-z_][A-Za-z0-9_.-]*`; otherwise all cues get `c1`, `c2`, ….
No styling or layout elements are emitted.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:ttm="http://www.w3.org/ns/ttml#metadata" xmlns:ttp="http://www.w3.org/ns/ttml#parameter" xml:lang="en" ttp:timeBase="media">
  <head>
    <metadata>
      <ttm:title>Lighthouse (synthetic sample)</ttm:title>
      <ttm:agent type="person" xml:id="agent1"><ttm:name type="full">MIRA</ttm:name></ttm:agent>
    </metadata>
  </head>
  <body>
    <div>
      <p xml:id="c1" begin="00:00:01.000" end="00:00:02.800" ttm:agent="agent1">Did you hear that?</p>
      <p xml:id="c3" begin="00:00:06.800" end="00:00:09.000">(A door creaks somewhere below.)</p>
    </div>
  </body>
</tt>
```

## Limits and errors

Parsing budgets (hard errors, distinct from the configurable checks):

| Budget | Value | Error |
|---|---|---|
| Input size | 8 MiB (`MAX_INPUT_BYTES`) | `InputTooLarge` |
| JSON nesting depth (including ignored fields) | 32 (`MAX_DEPTH`) | `Limit` at `input` |
| Cues | 100 000 (`MAX_CUES`) | `Limit` at `cues` |
| Lines per cue | 16 (`MAX_LINES_PER_CUE`) | `Limit` at `cues[i].lines` |
| Characters per line | 1024 (`MAX_LINE_CHARS`) | `Limit` at `cues[i].lines[j]` |
| Characters per `id` / `speaker` | 256 (`MAX_NAME_CHARS`) | `Limit` |
| Characters in `title` | 1024 (`MAX_TITLE_CHARS`) | `Limit` |
| Time values | ≤ 359 999 999 ms (`MAX_TIME_MS`) | `Limit` |

`Error` variants: `InputTooLarge`, `InvalidUtf8`, `InvalidJson {line, column}`
(malformed JSON or wrong shape; only the position is kept, serde's message is
dropped because it may quote input), `Invalid {path, reason}`,
`Limit {path, value, limit}`, `Config(reason)`, `Io(kind)`. `Display` never
contains input text or file names. Parsing is non-recursive on our side (serde
deserializes a flat struct; unknown fields are skipped iteratively by
serde_json) and additionally guarded by the depth budget.

## CLI

```text
subtitle-writer <in.json> [--format srt|vtt|ttml] [-o OUT] [--report REPORT.json] [--strict]
                [--max-cps N] [--max-line-length N] [--max-lines N]
                [--min-duration-ms N] [--max-duration-ms N]
```

- `<in.json>` is a file path or `-` for stdin.
- With `--format`, the subtitles go to stdout or to `-o OUT` (created or
  overwritten). The report goes to `--report REPORT.json` if given; otherwise a
  **non-empty** report is printed to stderr (nothing when clean).
- Without `--format`, the report goes to stdout (or to `--report`).
- `--strict`: exit status 1 when there is at least one violation. The subtitle
  output is still written.
- Exit status 2: usage error, unreadable or invalid input, invalid limits
  (zero, or `--min-duration-ms` greater than `--max-duration-ms`).

```text
cargo run --locked -- samples/synthetic.json --format vtt -o out.vtt --report report.json
cargo run --locked -- samples/synthetic.json --strict --max-cps 20
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

Tests are offline. `samples/synthetic.{srt,vtt,ttml,report.json}` are golden
files regenerated from `samples/synthetic.json` by the CLI and compared
byte-for-byte in `tests/cli.rs`. The sample is invented dialogue with three
speakers, a speaker-less cue, a Cyrillic cue, lines containing `<`, `>` and
`&`, and deliberate violations of every configurable check plus an overlap.

## Decisions

- **Non-positive durations and empty lines** are reported as violations by
  `check` (so a report can list them) but are *errors* for the writers, since
  SRT/WebVTT terminate a cue at an empty line and a zero-length `<p>` is
  meaningless. The CLI computes the report before writing, so `--strict`
  reports them first; without `--strict`, writing such a document fails with
  exit 2.
- **Cues are never reordered or merged.** `not_sorted`/`overlap` are reported,
  and output keeps input order; fixing timing is the caller's job.
- **`not_sorted` and `overlap` are exclusive** per cue: a cue starting before
  the previous *start* is `not_sorted` only; starting before the previous
  *end* (but not before its start) is `overlap`.
- **Speaker counts for nothing**: CPS and line length ignore the `NAME: ` /
  `<v NAME>` decoration, since the label is a presentation choice.
- **WebVTT `</v>` is omitted** at the end of the cue, as the specification
  allows; a multi-line cue therefore has the whole payload inside the voice
  span, which is the usual interpretation.
- **TTML ids**: cue ids are used as `xml:id` only when they are all valid
  ASCII NCNames and unique; otherwise every `<p>` gets `c<n>` so the document
  is always well-formed XML. Agent ids are always `agent<n>`.
- **No `ttm:agent` for speaker-less cues**, no `region`, no styles.
- **`-->` is rejected in text** at parse time rather than escaped, because
  there is no portable escape for it in SRT.
- **Hours are capped at 99** so every timecode is `HH:MM:SS` with two-digit
  hours in all three formats.

## Not supported

- Reading SRT/WebVTT/TTML (no parsers, no round-trip).
- Inline markup (`<i>`, `<b>`, ruby, colours), positioning, regions, styles,
  WebVTT `NOTE`/`STYLE` blocks, TTML `ttp:frameRate`/frames timebase, SMPTE
  timecodes.
- Automatic line breaking, merging or splitting cues, shifting times,
  frame-based timing.
- Grapheme-cluster counting: characters are Unicode scalar values, so a
  combining sequence counts per code point.

## External references (facts only)

- SubRip format as commonly documented (no formal specification):
  <https://en.wikipedia.org/wiki/SubRip>
- WebVTT: <https://www.w3.org/TR/webvtt1/> (W3C), file signature, cue
  identifiers, timing line, voice spans and escaping.
- TTML 1.0: <https://www.w3.org/TR/ttml1/> (`ttp:timeBase`, clock-time
  expressions, `ttm:agent`, `ttm:title`, `br`).
- IMSC 1.1 profile: <https://www.w3.org/TR/ttml-imsc1.1/>.
- Reading-speed and line-length defaults (17 CPS, 42 characters, 1–7 s,
  two lines) follow widely published subtitling guidelines; they are
  configurable.
