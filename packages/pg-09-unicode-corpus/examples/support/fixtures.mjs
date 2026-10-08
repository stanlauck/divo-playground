// SPDX-License-Identifier: MIT OR Apache-2.0

// These are invented strings. Clusters and nonterminal UAX #14 offsets are
// curated here, not discovered by the algorithms that the corpus checks.
export const definitions = [
  { id: "empty", tags: ["boundary"], clusters: [] },
  {
    id: "ascii-space",
    tags: ["ascii", "space"],
    clusters: ["A", " ", "B"],
    allowed: [2],
  },
  {
    id: "latin-combining",
    tags: ["combining"],
    clusters: ["a\u0301\u0327", "b"],
  },
  {
    id: "leading-combining",
    tags: ["combining"],
    clusters: ["\u0301\u0327", "x"],
  },
  { id: "precomposed", tags: ["normalization"], clusters: ["\u00e9"] },
  { id: "decomposed", tags: ["normalization"], clusters: ["e\u0301"] },
  {
    id: "crlf-cr-lf",
    tags: ["mandatory", "crlf"],
    clusters: ["A", "\r\n", "B", "\n", "C", "\r", "D"],
    required: [3, 5, 7],
  },
  {
    id: "vertical-tab",
    tags: ["mandatory"],
    clusters: ["A", "\u000b", "B"],
    required: [2],
  },
  {
    id: "form-feed",
    tags: ["mandatory"],
    clusters: ["A", "\u000c", "B"],
    required: [2],
  },
  {
    id: "next-line",
    tags: ["mandatory"],
    clusters: ["A", "\u0085", "B"],
    required: [2],
  },
  {
    id: "line-paragraph-separators",
    tags: ["mandatory"],
    clusters: ["A", "\u2028", "B", "\u2029", "C"],
    required: [2, 4],
  },
  {
    id: "tab",
    tags: ["space"],
    clusters: ["A", "\t", "B"],
    allowed: [2],
  },
  {
    id: "multiple-spaces",
    tags: ["space"],
    clusters: ["A", " ", " ", "B"],
    allowed: [3],
  },
  {
    id: "zero-width-space",
    tags: ["space", "invisible"],
    clusters: ["A", "\u200b", "B"],
    allowed: [2],
  },
  {
    id: "word-joiner",
    tags: ["no-break", "invisible"],
    clusters: ["A", "\u2060", "B"],
  },
  {
    id: "nbsp",
    tags: ["no-break", "russian", "nbsp"],
    clusters: ["А", "\u00a0", "Б"],
  },
  {
    id: "narrow-nbsp",
    tags: ["no-break", "russian", "narrow-nbsp"],
    clusters: ["1", "0", "\u202f", "м"],
  },
  {
    id: "soft-hyphen",
    tags: ["russian", "soft-hyphen", "invisible"],
    clusters: ["п", "о", "\u00ad", "л", "е"],
    allowed: [3],
  },
  {
    id: "russian-hyphen",
    tags: ["russian", "hyphen"],
    clusters: ["а", "л", "ь", "ф", "а", "-", "б", "е", "т", "а"],
    allowed: [6],
  },
  {
    id: "nonbreaking-hyphen",
    tags: ["russian", "hyphen", "no-break"],
    clusters: ["а", "\u2011", "б"],
  },
  {
    id: "russian-spaced-em-dash",
    tags: ["russian", "dash"],
    clusters: ["А", " ", "—", " ", "Б"],
    allowed: [2, 4],
  },
  {
    id: "russian-em-dash",
    tags: ["russian", "dash"],
    clusters: ["А", "—", "Б"],
    allowed: [1, 2],
  },
  {
    id: "russian-en-dash",
    tags: ["russian", "dash"],
    clusters: ["А", "–", "Б"],
    allowed: [2],
  },
  {
    id: "paired-em-dash",
    tags: ["russian", "dash"],
    clusters: ["А", "—", "—", "Б"],
    allowed: [1, 3],
  },
  {
    id: "cjk-punctuation",
    tags: ["cjk", "punctuation"],
    clusters: ["山", "水", "。", "花"],
    allowed: [1, 3],
  },
  {
    id: "japanese-brackets",
    tags: ["cjk", "punctuation"],
    clusters: ["「", "あ", "い", "」", "。", "う"],
    allowed: [2, 5],
  },
  {
    id: "hangul-jamo",
    tags: ["cjk", "hangul"],
    clusters: ["\u1100\u1161\u11a8", "\u1102\u1161"],
    allowed: [3],
  },
  {
    id: "devanagari-matra",
    tags: ["devanagari", "combining"],
    clusters: ["कि", " ", "ना"],
    allowed: [3],
  },
  {
    id: "devanagari-conjunct",
    tags: ["devanagari", "conjunct"],
    clusters: ["क्षि", " ", "ग"],
    allowed: [5],
  },
  {
    id: "devanagari-zwj-conjunct",
    tags: ["devanagari", "conjunct", "zwj"],
    clusters: ["क्\u200dष", " ", "ग"],
    allowed: [5],
  },
  {
    id: "emoji-supplementary",
    tags: ["emoji", "supplementary"],
    clusters: ["😀", " ", "X"],
    allowed: [3],
  },
  {
    id: "emoji-modifier",
    tags: ["emoji", "modifier"],
    clusters: ["👍🏽", "X"],
    allowed: [4],
  },
  {
    id: "emoji-family",
    tags: ["emoji", "zwj"],
    clusters: ["👨‍👩‍👧‍👦", " ", "X"],
    allowed: [12],
  },
  {
    id: "emoji-professions",
    tags: ["emoji", "zwj", "modifier"],
    clusters: ["👩🏽‍🚀", "👨‍💻"],
    allowed: [7],
  },
  {
    id: "regional-indicator-parity",
    tags: ["emoji", "regional-indicator"],
    clusters: ["🇦🇧", "🇨🇩", "🇪"],
    allowed: [4, 8],
  },
  {
    id: "emoji-keycaps",
    tags: ["emoji", "combining", "keycap"],
    clusters: ["1\ufe0f\u20e3", "2\ufe0f\u20e3"],
  },
  {
    id: "emoji-tags",
    tags: ["emoji", "supplementary", "tag-sequence"],
    clusters: ["🏴\u{e0061}\u{e0062}\u{e007f}", "X"],
    allowed: [8],
  },
  {
    id: "emoji-variation-selector",
    tags: ["emoji", "variation-selector"],
    clusters: ["✈\ufe0f", "X"],
    allowed: [2],
  },
  {
    id: "supplementary-cjk",
    tags: ["cjk", "supplementary"],
    clusters: ["\u{20000}", "中"],
    allowed: [2],
  },
  {
    id: "supplementary-combining",
    tags: ["combining", "supplementary"],
    clusters: ["A\u{1d165}", "B"],
  },
  {
    id: "space-combining-rule-boundary",
    tags: ["combining", "uax14-vs-uax29"],
    clusters: [" \u0301", "A"],
    // LB9 excludes a combining mark after SP; LB18 allows the break at 1.
    // GB9 keeps that same mark in the preceding extended grapheme.
    allowed: [1],
  },
  {
    id: "embedded-bom-word-joiner",
    tags: ["no-break", "invisible", "normalization"],
    clusters: ["A", "\ufeff", "B"],
  },
  {
    id: "arabic-logical-only",
    tags: ["rtl", "arabic", "combining"],
    clusters: ["ا\u064e", " ", "ب"],
    allowed: [3],
    deferred: ["visual_cursor", "bidi_reordering"],
  },
  {
    id: "hebrew-mixed-logical-only",
    tags: ["rtl", "hebrew", "combining"],
    clusters: ["א\u05b0", " ", "B", "1"],
    allowed: [3],
    deferred: ["visual_cursor", "bidi_reordering"],
  },
];

export function makeCorpus() {
  const encoder = new TextEncoder();
  return {
    version: 1,
    unicodeVersion: "17.0.0",
    indexEncodings: ["utf16", "utf8"],
    cursorMode: "logical",
    lineBreakMode: "uax14-default",
    normalization: "none",
    cases: definitions.map((definition) => {
      const text = definition.clusters.join("");
      const at = (utf16) => ({
        utf16,
        utf8: encoder.encode(text.slice(0, utf16)).length,
      });
      let offset = 0;
      const graphemes = definition.clusters.map((part) => {
        const start = at(offset);
        offset += part.length;
        return { text: part, start, end: at(offset) };
      });
      const boundaries = [0, ...graphemes.map((part) => part.end.utf16)];
      const scalarOffsets = [0];
      let scalarOffset = 0;
      for (const scalar of text) {
        scalarOffset += scalar.length;
        scalarOffsets.push(scalarOffset);
      }
      const cursor = scalarOffsets.map((offset) => ({
        at: at(offset),
        previous:
          boundaries.findLast((boundary) => boundary < offset) === undefined
            ? null
            : at(boundaries.findLast((boundary) => boundary < offset)),
        next:
          boundaries.find((boundary) => boundary > offset) === undefined
            ? null
            : at(boundaries.find((boundary) => boundary > offset)),
      }));
      const required = new Set([...(definition.required ?? []), text.length]);
      const lineBreaks = [
        ...(definition.allowed ?? []),
        ...(definition.required ?? []),
        text.length,
      ]
        .sort((a, b) => a - b)
        .map((offset) => ({ at: at(offset), required: required.has(offset) }));
      return {
        id: definition.id,
        tags: definition.tags,
        text,
        ...(definition.deferred ? { deferred: definition.deferred } : {}),
        expected: {
          graphemeCount: graphemes.length,
          graphemes,
          cursor,
          lineBreaks,
        },
      };
    }),
  };
}
