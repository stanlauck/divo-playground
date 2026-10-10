<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Fountain conformance corpus and runner

This package is a small, synthetic corpus for checking Fountain parsers. Each `.fountain` file has an `.expected.json` companion. The expected files use **Neutral element JSON v1**, a deliberately format-neutral representation of title-page entries and screenplay elements.

The cases are based on the public [Fountain syntax reference](https://fountain.io/syntax). All words in this repository are invented for tests.

## Neutral element JSON v1

```json
{"version":1,"title_page":[{"key":"Title","value":"..."}],"elements":[{"type":"action","text":"..."}]}
```

`title_page` is an array (empty when there is no title page). Every element has `type` and `text`. Text is the raw content after a structural marker is removed; emphasis markup remains verbatim. Optional fields are `scene_number`, `depth` (section nesting), `dual` (the second character in a dual block), and `forced`. Notes and boneyards contain their inner text. A dual block is ordered as `dual_dialogue_begin`, its two character/dialogue groups, and `dual_dialogue_end`. Parsers may add a `styled` field; the runner drops that presentation-only field before strict comparison.

| Type | Meaning |
| --- | --- |
| `scene_heading` | Scene heading, with optional `scene_number` |
| `action` | Action text, including intentional line breaks |
| `character` | Character cue; `dual: true` marks the second cue in a dual block |
| `parenthetical` | Parenthetical below a character cue |
| `dialogue` | Dialogue text |
| `dual_dialogue_begin`, `dual_dialogue_end` | Dual-dialogue delimiters |
| `transition` | Transition, including forced transitions |
| `centered` | Text wrapped in `>` and `<` |
| `lyric` | A line beginning with `~` |
| `page_break` | A line of three or more `=` characters; `text` is empty |
| `section` | A `#` heading, with `depth` |
| `synopsis` | A line beginning with `=` |
| `note` | Content inside `[[` and `]]` |
| `boneyard` | Content inside `/*` and `*/` |

## Running a parser

Build with Node 22 and TypeScript, then run a parser command for every case:

```sh
npm install
npm test
node dist/cli.js run --parser 'node examples/your-parser.mjs'
```

The command receives normalized Fountain text on stdin and must print one JSON document on stdout. A file-mode command may contain `{file}`; the placeholder is replaced with the case's path and stdin is still available. `examples/echo-expected.mjs` is a tiny self-checking parser, while `examples/wrong-parser.mjs` demonstrates a readable failure.

Options:

- `--ignore note,boneyard` removes those types from both lists before comparison.
- `--loose-whitespace` changes runs of whitespace in `text` to one space.
- `--only <case-id>` runs one case.
- `--json` prints machine-readable case and summary objects.
- `node dist/src/cli.js list` (or `run --list`) lists case IDs.

The command exits `0` when all selected cases pass, `1` when any case fails, and `2` for command-line or parser-output errors.

The library exports `loadCorpus()`, `compareElements(expected, actual, options)`, `validateExpected()`, and `normalizeFountain()` from `dist/index.js`.

## Cases

`01` title-page keys, blank `Title:`, and indented multi-line values; `02` scene-heading forms, lowercase and forced headings, dots, and scene numbers; `03` action, forced action, indentation, and connected two-space blank lines; `04` character cues, extensions, forced mixed-case cues, and uppercase action; `05` parentheticals and multi-paragraph dialogue; `06` dual dialogue; `07` transitions; `08` centered text; `09` lyrics; `10` page breaks; `11` sections and synopses; `12` notes; `13` boneyards; `14` emphasis and escaped markup; `15` BOM/CRLF normalization; `16` Unicode scripts and combining marks; `17-empty`, `17-blanks`, and `17-trailing` empty/whitespace edges; `18` a kitchen-sink combination.

## Adding a case

Add a paired `corpus/<id>.fountain` and `corpus/<id>.expected.json`, keep text synthetic, and run `npm test`. Add the rule to `SPEC-NOTES.md` and the case list above. JSON files must have `version: 1`, a `title_page` array, and an `elements` array with valid element fields.

## Decisions

The runner normalizes a UTF-8 BOM and CRLF/CR line endings before invoking a parser. This makes the line-ending case compare equal to its LF equivalent. Inline notes and boneyards are emitted in source order as separate elements, with adjacent action fragments retained. Page breaks have empty text. Comparison is strict deep equality of element objects after the requested filters.
