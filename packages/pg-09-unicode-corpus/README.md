# Unicode corpus and checks (PG-09)

Synthetic Unicode regression vectors and a TypeScript checking API. Unicode
**17.0.0**, corpus format **1**, licence **MIT OR Apache-2.0**.

This package checks extended grapheme clusters (UAX #29), **logical**
`previous` / `next` cursor movement, and untailored line-break opportunities
(UAX #14). Every position contains both a UTF-16 code-unit offset and a UTF-8
byte offset. The same JSON vectors can check JavaScript and Rust implementations.

It does **not** render text, shape glyphs, perform BiDi reordering, move a cursor
visually left/right, measure fonts, wrap to a width, or paginate. Two Arabic and
Hebrew cases explicitly defer `visual_cursor` and `bidi_reordering`; their
logical indexing and line-breaking checks still run.

## Corpus

`corpus/corpus.json` contains **44 invented cases**, not prose from existing
scripts or copied Unicode test files:

- Arabic/Hebrew combining marks and mixed RTL/Latin text, logical order only.
- Devanagari matras, conjuncts, and a ZWJ conjunct.
- CJK punctuation, Japanese brackets, Hangul Jamo and supplementary ideographs.
- Combining marks, leading marks, composed/decomposed forms and astral marks.
- U+00A0 NBSP, U+202F narrow NBSP, U+00AD soft hyphen, Russian hyphens,
  nonbreaking hyphens, en/em dashes and paired em dashes.
- Supplementary emoji, skin tones, family/profession ZWJ sequences, regional
  indicator parity, keycaps, tag sequences and variation selectors.
- CRLF/CR/LF, other mandatory separators, tabs, spaces, zero-width space,
  word joiner, embedded U+FEFF and empty text.

Expected clusters and nonterminal line-break positions are hand-curated in
`examples/support/fixtures.mjs`. The generator only expands those definitions
into paired positions using native UTF-8 encoding. It **does not import the
Unicode backends** or snapshot their answers. The committed JSON is tested
against those independent definitions.

This is a focused regression corpus, **not** the complete Unicode conformance
suite. No full UAX #9, locale-specific tailoring or dictionary segmentation
conformance is claimed.

### Format

`corpus/schema.json` describes the structure (JSON Schema 2020-12). The TS
validator additionally checks original-text slices, scalar alignment, byte
agreement, contiguous expected graphemes, unique IDs and complete cursor probes.

```json
{
  "version": 1,
  "unicodeVersion": "17.0.0",
  "indexEncodings": ["utf16", "utf8"],
  "cursorMode": "logical",
  "lineBreakMode": "uax14-default",
  "normalization": "none",
  "cases": [
    {
      "id": "tiny",
      "tags": ["boundary"],
      "text": "",
      "expected": {
        "graphemeCount": 0,
        "graphemes": [],
        "cursor": [
          { "at": { "utf16": 0, "utf8": 0 }, "previous": null, "next": null }
        ],
        "lineBreaks": [{ "at": { "utf16": 0, "utf8": 0 }, "required": true }]
      }
    }
  ]
}
```

All positions refer to the **original** text. No NFC/NFD normalization,
whitespace trimming, soft-hyphen removal, replacement of invalid text, or
newline conversion occurs.

`cursor` contains one probe at **every Unicode scalar boundary**, including
positions inside an extended grapheme. `previous` / `next` mean the strictly
smaller/larger grapheme boundary. At the start/end, a missing neighbour is
`null`, not a clamped offset. A probe inside a cluster moves outward to that
cluster's start/end. Offsets inside a surrogate pair or UTF-8 scalar are errors.

`lineBreaks` contains allowed (`required: false`) and mandatory
(`required: true`) opportunities. It never includes start-of-text for nonempty
text. End-of-text is always mandatory (LB3), including `{0, 0}` for empty text.
Positions are after any hard-break characters; CRLF has one break after LF.
No discretionary hyphen glyph is inserted for U+00AD.

**UAX #14 and UAX #29 are not identical boundary sets.** For example, in
`" \u0301A"`, untailored UAX #14 allows a break after the space while UAX #29
keeps the combining mark in that space's grapheme. The corpus preserves this
distinction. A higher-level layout engine may impose additional grapheme
constraints; this package does not silently tailor the UAX #14 result.

## API

Build before importing. The root export is ESM with TypeScript declarations.

```ts
import {
  UnicodeText,
  analyzeText,
  parseCorpus,
  checkCorpus,
} from "@divo-playground/pg-09-unicode-corpus";

const text = new UnicodeText("A👩🏽‍🚀Б");
const inside = text.positionAt(3, "utf16"); // { utf16: 3, utf8: 5 }
text.move(inside, "previous"); // { utf16: 1, utf8: 1 }
text.move(inside, "next"); // { utf16: 8, utf8: 16 }
text.positionAt(16, "utf8"); // { utf16: 8, utf8: 16 }
text.graphemeCount; // 3
text.graphemes; // original slices and paired start/end positions
text.lineBreaks(); // paired positions and required flags
analyzeText(text.text); // complete portable analysis, including all cursor probes

// json is the UTF-8 corpus decoded to a JS string:
const corpus = parseCorpus(json);
const report = checkCorpus(corpus);
```

`validateCorpus(unknown)` accepts already parsed data. Both validation functions
return a detached, deeply frozen corpus. `checkCorpus` revalidates its input and
reports per-case, per-field mismatches; it does not rewrite expected answers.
Unknown fields and unsupported versions/profiles are rejected.

`checkCorpus(corpus, analyzer)` accepts a synchronous custom backend returning
the exported `TextAnalysis` shape. A Rust bridge can return that shape with
paired offsets. Native grapheme algorithms and Unicode versions must be
compatible with this corpus; the checker reports a version mismatch rather than
silently updating expectations. Object-property order does not matter.
Exceptions from a custom analyzer produce a sanitized failed case; other cases
still run. Analyzer code itself is trusted, not sandboxed.

`UnicodeError.code` distinguishes invalid text/positions/corpus, exceeded
limits, unsupported native Unicode versions and backend invariant failures.
Results and index arrays are immutable. Index construction uses linear space;
offset conversion and cursor movement use binary searches rather than encoding
every text prefix. `analyzeText` intentionally materializes the full result.

## Unicode backends and runtime

- `unicode-segmenter@0.17.3`: Unicode 17.0 UAX #29 extended grapheme clusters.
- `@cto.af/linebreak@4.0.3`: Unicode 17.0 UAX #14 rules/tables.
- Both are MIT-licensed dependencies. Their integrity hashes and transitive
  dependencies are pinned by `package-lock.json`.

The line-break backend coalesces LB9 characters into the base character and
can then miss LB8a after a ZWJ. This package adds a **narrow normative LB8a
correction**, before that rule, using the actual last consumed U+200D. Regression
tests keep family/profession emoji intact without filtering all line breaks
through UAX #29 or changing other UAX #14 opportunities.

Use **Node 24.18+ with native Unicode 17.0 properties** (tested on 24.18.0).
The line-break backend also uses native Unicode-property regular expressions;
the API explicitly rejects other native Unicode versions instead of pretending
its tables alone make it version-independent. The package is a Node checking
library, not a browser bundle. UTF-16 offsets and portable corpus data remain
suitable for web-side implementations.

Limits: 100,000 UTF-16 code units per text; 1,000,000 total corpus text units;
256 cases; 16,777,216 UTF-16 units in corpus JSON. CLI input is a regular file,
at most 3 times the JSON unit limit in bytes, read with an initial-size bound.
Invalid UTF-8 is rejected by a fatal decoder. JSON parsing uses `JSON.parse`;
like that parser, repeated JSON object keys use the last value. These limits
are not a CPU/RSS sandbox, especially for a supplied custom analyzer.

## Offline checks and examples

Install trusted locked dependencies once; tests themselves make no network calls:

```sh
npm ci --ignore-scripts --no-audit --no-fund
npm --offline test
npm --offline run typecheck
npm --offline run format:check
npm --offline run check:corpus
```

The CLI writes a JSON `CheckReport`. Exit code is zero only when every case
passes, and nonzero for an invalid corpus or mismatches:

```sh
node dist/examples/check.js corpus/corpus.json
```

To reproduce the corpus at a **new** path (existing files are never overwritten):

```sh
node examples/make-corpus.mjs target/reproduced-corpus.json
```

Create the parent directory first. Generation and all examples are local/offline.
The package has no rendering or editor integration and executes no input scripts.

## Licence

Original package code, schema and synthetic corpus:
**MIT OR Apache-2.0**. See `LICENSE-MIT` and `LICENSE-APACHE`.
