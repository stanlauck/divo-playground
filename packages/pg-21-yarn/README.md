<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# PG-21 — Yarn Spinner 2 parser

`pg-21-yarn` reads Yarn Spinner 2 `.yarn` scripts and emits one **neutral
dialogue-graph** JSON document: nodes, edges, variables, and a findings report.
It is a reader, not a runtime — it never evaluates an expression, never resolves
a function call, and never invents a value the script did not state.

The command-line front end is `yarn2graph`.

## Provenance

Written clean room. The Yarn Spinner 2.5 script-format documentation is the only
reference used; no code was read or copied from the Yarn Spinner compiler or any
other project. All sample and fixture scripts in this package are synthetic and
were written for it.

## Build and test

Everything runs offline. `serde` and `serde_json` are the only dependencies and
both are pinned to exact versions.

```console
cargo build --offline
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Command line

```console
yarn2graph <in.yarn>... [-o out.json] [--strict]
```

| Option | Meaning |
| --- | --- |
| `-o <path>` | Write the JSON document to `<path>` instead of stdout. `-o -` means stdout. |
| `--strict` | Fail on warnings too, not only on errors. |
| `-h`, `--help` | Print usage and exit 0. |

Several input files are merged into one document; `nodes[].source` indexes
`sources[]`, so a `<<jump>>` may target a node declared in another file.

Findings go to **stderr** as `file:line:col: severity[code]: message`, followed
by a one-line summary. The JSON document is written either way, because a
best-effort graph is still useful when the script has problems.

| Exit code | Meaning |
| --- | --- |
| 0 | Nothing above the threshold: no errors, or with `--strict` no findings at all. |
| 1 | Findings above the threshold. |
| 2 | Usage error, unreadable input, non-UTF-8 input, or a hard limit was hit. |

## The JSON document

Version 1. Fields that carry no information are omitted rather than emitted as
`null` or `[]`, which keeps golden files small and diffs readable.

```text
{
  "version": 1,
  "sources": ["demo.yarn"],       // input names in command-line order
  "start_node": "Start:1",        // node id to enter, or null
  "titles": [Title],              // one per Yarn node block, in source order
  "nodes": [Node],                // one per statement, in source order
  "edges": [Edge],                // in traversal order
  "variables": [Variable],        // every <<declare>>, in source order
  "errors": [Finding]             // errors and warnings together
}
```

### `Title` — a Yarn node block

```text
{
  "id": "Start",                  // unique key; prefixes every statement id here
  "title": "Start",               // the title: header value, or "untitled"
  "source": 0,
  "tags": ["intro"],              // omitted when the header was absent or empty
  "meta": {"position": "120,-40"},// every other header, values kept as written
  "entry": "Start:1",             // first statement, or null for an empty body
  "line": 1, "col": 1
}
```

`title` and `tags` are understood headers. Any other `key: value` line before
`---` lands in `meta` verbatim — `position`, `description`, engine-specific keys
— so nothing a script declares is thrown away.

### `Node` — one statement

```text
{
  "id": "Start:1",                // "<title id>:<ordinal>", unique document-wide
  "source": 0,
  "title": "Start",               // owning Title.id
  "kind": "line",                 // line | options | command | jump | conditional
  "speaker": "Ada",               // Character: prefix, when the line had one
  "text": "Hello [wave]there[/wave].",
  "line_id": "1a0001",            // value of #line:, without the "line:" prefix
  "tags": ["warm"],               // every other hashtag, without the leading #
  "markup": true,                 // present only when text holds a [markup] tag
  "condition": "$gold > 10",      // trailing <<if>> on an option
  "option_group": 1,              // 1-based choice group inside the Yarn node
  "parent": "Start:6",            // enclosing options or conditional node
  "clause": "elseif 1",           // which clause, when parent is a conditional
  "command": Command,             // for command, jump and conditional nodes
  "line": 6, "col": 6
}
```

`text`, `condition` and `command.expression` are verbatim. `{$name}`
interpolations and `[markup]` tags are preserved exactly as written, because
resolving them needs a runtime the reader does not have. `markup: true` is a
convenience flag for consumers that want to know whether a text needs a markup
pass at all.

`col` is the 1-based **character** offset of the statement's own content: past
the indentation, past `-> `, past a `Character: ` prefix. It points at the text,
which is what an editor wants to jump to.

### `Command`

```text
{
  "name": "set",                  // the command word exactly as written
  "args": "$gold += 5",           // everything after it, verbatim and trimmed
  "variable": "gold",             // set / declare: name without the $
  "operator": "+=",               // = += -= *= /= or to
  "expression": "5",              // right-hand side or condition, verbatim
  "target": "Ledger",             // jump: destination title
  "clauses": [Clause]             // conditional: one per if / elseif / else
}
```

`name` matching is case-insensitive; the value is not normalised, so a script
that writes `<<JUMP x>>` still reads `JUMP`.

### `Clause`

```text
{
  "keyword": "else_if",           // if | else_if | else
  "label": "elseif 1",            // matches Node.clause and Edge.label
  "expression": "$gold > 10",     // absent for else
  "body": ["Ledger:3"],           // node ids entered when the clause is taken
  "line": 29, "col": 1
}
```

### `Edge`

```text
{
  "id": "edge-1",                 // generated, in traversal order
  "kind": "branch",               // flow | option | branch | jump
  "source": "Ledger:2",
  "target": "Ledger:3",
  "index": 0,                     // 0-based position in edges[]
  "label": "if"                   // branch edges only: the clause label
}
```

| Kind | Connects |
| --- | --- |
| `flow` | One statement to the next inside a block. |
| `option` | The statement before a choice group to each of its options. |
| `branch` | A `conditional` node into one of its clause bodies. |
| `jump` | A `jump` node to the entry node of the target Yarn node. |

Every clause body rejoins the statement that follows `<<endif>>`, so an
if/elseif/else chain is a diamond, not a set of dead ends. `<<stop>>` ends
dialogue and therefore has **no** outgoing edge. An option whose body is empty
links straight to whatever follows the choice group.

### `Variable`

```text
{
  "name": "gold",                 // without the leading $
  "type": "int",                  // bool | int | float | string | unknown
  "declared_as": "number",        // the word written in "as <type>", if any
  "value": 0,                     // JSON literal, or verbatim text when unknown
  "description": "Gold the player carries.",
  "source": 0, "title": "Start", "line": 9, "col": 1
}
```

`type` comes from an explicit `as <type>` suffix when there is one, otherwise
from the shape of the initial literal. `bool`, `string`, `number`, `int`,
`float`, `boolean` and `integer` are recognised, case-insensitively; anything
else is a warning and the inferred type is kept. When the initial value is not a
literal — `<<declare $x = $y + 1>>` — the type is `unknown` and `value` holds the
text verbatim.

`value` is never coerced to `type`. `<<declare $odd = 7 as string>>` records
`type: "string"` with `value: 7` and an `invalid_variable_type` warning, because
silently rewriting `7` into `"7"` would hide the mistake from the script author.

`///` comments immediately above a `<<declare>>` become `description`.

Only `<<declare>>` produces a variable. `<<set>>` on a name that was never
declared is an implicit declaration in Yarn Spinner and is deliberately **not**
reported here: the reader records what the script states, and guessing a type
from an assignment would be inventing information.

### `Finding`

```text
{
  "file": "demo.yarn",            // the source name exactly as passed in
  "line": 4, "col": 6,
  "code": "duplicate_line_id",
  "severity": "error",            // error | warning
  "message": "`#line:dup1` is already used by demo.yarn line 3 column 6"
}
```

`errors[]` is sorted by source, then position, then code — the same input always
produces byte-identical JSON.

## Yarn construct → node kind

| Yarn construct | Node kind | Notes |
| --- | --- | --- |
| `Ada: Hello. #line:1a1 #warm` | `line` | `speaker`, `text`, `line_id`, `tags` |
| `Hello.` (no speaker) | `line` | `speaker` omitted |
| `-> Take the coin.` | `options` | one node per option; `option_group` numbers the choice group |
| `-> Ada: Take the coin.` | `options` | options may carry a `Character:` prefix |
| `-> Buy it. <<if $gold >= 10>>` | `options` | the condition goes to `condition`, not `text` |
| indented lines under `->` | `line` / `command` / … | `parent` points at the option; nesting follows indentation |
| `<<jump Ledger>>` | `jump` | `command.target`; becomes a `jump` edge |
| `<<set $gold += 5>>` | `command` | `variable`, `operator`, `expression` |
| `<<set $gold to 5>>` | `command` | `to` is recorded as the operator |
| `<<declare $gold = 0 as number>>` | `command` | also appends a `variables[]` entry |
| `<<if>> … <<elseif>> … <<else>> … <<endif>>` | `conditional` | one node for the whole block; `command.clauses` lists the branches |
| `<<stop>>` | `command` | built-in; no outgoing edge |
| `<<wait 2>>` | `command` | built-in |
| `<<fadeOut 2>>` | `command` | any other command; also a `unknown_command` **warning** |
| `// comment` | *(none)* | skipped |
| `/// doc comment` | *(none)* | kept as `description` when it precedes a `<<declare>>` |
| `title:`, `tags:`, `---`, `===` | *(none)* | become `titles[]` entries and block boundaries |

A `conditional` node is a single node for the whole `<<if>>` block, not one per
clause. Its clause bodies carry `parent` = the conditional node's id and
`clause` = the clause label, which is what makes the diamond in `edges[]`
readable.

## Node ids and scopes

Statement ids are `"<title id>:<ordinal>"`, where the ordinal counts statements
in source order starting at 1. Ids are opaque strings; a consumer should never
parse one.

A `title:` block that repeats a title already seen gets the scope
`"Name#2"`, `"Name#3"`, … and a `duplicate_node_title` error, so every id in the
document stays unique instead of silently colliding. `titles[].id` is that scope
and `titles[].title` is what the script actually wrote — a `<<jump Name>>` still
resolves to the first block named `Name`, which is what Yarn Spinner does.

`start_node` is the entry of the first block titled exactly `Start`, or of the
first block that has a body when there is no such block, or `null` when nothing
is parseable. A `Start` block with an empty body does not hide the first node a
runtime could actually enter.

## Lines, hashtags and markup

A line is scanned left to right, once:

- `Character:` is split off as `speaker` only when the name has no whitespace,
  starts with a letter or `_`, and holds nothing else but letters, digits and
  `_ - . '`. That keeps `http://a.com` and `10:30 in the morning` as prose
  rather than as speakers.
- The hashtag region starts at the first `#` that both begins a token and is
  followed by a non-space character. A bare `#` in dialogue (`Press # to
  continue.`) is therefore kept as text instead of swallowing the rest of the
  line.
- Inside the region, `#line:` sets `line_id` and every other hashtag becomes a
  `tags[]` entry. A token that does not start with `#` is a `malformed_hashtag`
  error; a second `#line:` is `duplicate_line_id`; an empty one is
  `empty_line_id`.
- `//` ends the line, whether it appears before the hashtag region or inside it.
- `\#` is an escaped literal `#`, `\[` and `\]` are escaped literal brackets.
- `#` and `//` are ignored inside `"quoted text"` and inside `{$interpolation}`.

`markup` is true when `text` holds at least one `[tag]` … `[/tag]` pair: the
bracket must open a name starting with an ASCII letter or `_`, and the tag must
close on the same line. `[wave]`, `[/wave]` and `[bold color="red"]` all
qualify; a bare `[` does not.

## Findings

| Code | Severity | Raised when |
| --- | --- | --- |
| `bad_command_syntax` | error | `<<jump>>` has no target, or a `set`/`declare` has no `$name = value` shape |
| `content_outside_node` | error | prose before any `title:` block |
| `duplicate_header_key` | warning | the same header key appears twice in one block; the first wins |
| `duplicate_line_id` | error | a `#line:` id already used earlier in the run |
| `duplicate_node_title` | error | a `title:` repeats an earlier one |
| `duplicate_variable` | error | a `$name` is declared more than once; the first declaration wins |
| `empty_line_id` | error | `#line:` with nothing after the colon |
| `invalid_variable_type` | warning | `as <type>` is not a known type, or does not match the initial literal, or the literal is not recognisable |
| `malformed_hashtag` | error | a token inside the hashtag region does not start with `#` |
| `missing_header_delimiter` | error | headers are not followed by `---` |
| `missing_jump_target` | error | `<<jump X>>` names a title that does not exist in any input |
| `missing_node_title` | error | a block has no usable `title:` header; the `untitled` scope is assigned |
| `nesting_too_deep` | error | option/conditional nesting passed `max_block_depth`; the rest of the node is dropped |
| `option_after_node_end` | error | a `->` line outside any node body |
| `trailing_content_after_command` | warning | `<<else>>` carries a condition |
| `unbalanced_conditional` | error | `<<if>>` with no `<<endif>>`, or a clause keyword after `<<else>>`, or a stray `<<elseif>>`/`<<else>>`/`<<endif>>` |
| `unexpected_header_delimiter` | error | `---` inside a node body |
| `unexpected_node_end` | error | `===` with no open node |
| `unknown_command` | warning | a command that is not one of `declare`, `else`, `elseif`, `endif`, `if`, `jump`, `set`, `stop`, `wait`; still emitted as a `command` node |
| `unterminated_command` | error | `<<` with no matching `>>` |

Warnings never fail a default run. An unknown command is a warning on purpose:
game-specific commands are normal in Yarn Spinner, and a reader that rejected
them would be useless on real projects.

## Recovery and limits

Parsing is best-effort. A malformed line is reported and skipped; the rest of
the document is still produced, so one broken line cannot hide the other two
hundred. Where a construct is ambiguous the reader keeps text rather than
dropping it.

A body still open at end of file is closed silently — the trailing `===` is
optional in Yarn Spinner.

Recursion is bounded, and so is memory. `ParseOptions` sets the limits:

| Limit | Default | Hard failure |
| --- | --- | --- |
| `max_input_bytes` | 8 MiB per file | `Error::TooLarge` |
| `max_nodes` | 200 000 statements | `Error::TooManyNodes` |
| `max_findings` | 10 000 findings | `Error::TooManyFindings` |
| `max_block_depth` | 64 nesting levels | `nesting_too_deep` finding |

Exceeding `max_findings` is a hard error rather than a silent truncation: a
report that quietly dropped findings would look clean.

A tab counts as 4 columns when measuring indentation. Line endings may be `\n`
or `\r\n`, and a leading UTF-8 byte-order mark is ignored.

## Library use

```rust
use pg_21_yarn::{parse, to_json};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let script = "title: Start\n---\nAda: Hello. #line:1a0001\n";
let graph = parse("demo.yarn", script)?;

assert_eq!(graph.start_node.as_deref(), Some("Start:1"));
assert_eq!(graph.nodes[0].line_id.as_deref(), Some("1a0001"));
assert_eq!(graph.error_count(), 0);
assert!(to_json(&graph)?.starts_with("{\n  \"version\": 1,"));
# Ok(())
# }
```

`parse_sources` takes several in-memory sources, `read_files` reads them from
disk under the same limits, and `to_json` serialises with LF endings and a
trailing newline so golden files compare identically on every platform.

## Sample

`samples/synthetic.yarn` is a hand-written synthetic script exercising headers,
speakers, hashtags, interpolation, markup, nested options, a conditional option,
`<<declare>>`, `<<set>>`, an if/elseif/else chain, `<<stop>>`, both jump
directions and one unknown command:

```console
cargo run --offline --bin yarn2graph -- samples/synthetic.yarn
```

## Tests

`cargo test --offline` runs unit tests, golden tests over every
`tests/fixtures/*.yarn` against its `*.json` sibling, and a property test that
generates random nested option trees and asserts each option ends up with
exactly one parent. No test touches the network, and fixtures carry no machine
paths — each is parsed under its own file name only.

## Licence

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
