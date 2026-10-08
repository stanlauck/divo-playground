// SPDX-License-Identifier: MIT OR Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_TEXT_UTF16,
  UNICODE_VERSION,
  UnicodeError,
  UnicodeText,
  analyzeText,
  scalarPositions,
} from "../dist/src/index.js";
import { UnicodeText as ExportedUnicodeText } from "@divo-playground/pg-09-unicode-corpus";

const hasCode = (code) => (error) =>
  error instanceof UnicodeError && error.code === code;

test("public ESM export resolves to the tested API", () => {
  assert.equal(ExportedUnicodeText, UnicodeText);
  assert.equal(new ExportedUnicodeText("A😀中").graphemeCount, 3);
});

test("paired offsets preserve supplementary scalars and combining marks", () => {
  const index = new UnicodeText("A😀e\u0301中");
  assert.deepEqual(index.positions, [
    { utf16: 0, utf8: 0 },
    { utf16: 1, utf8: 1 },
    { utf16: 3, utf8: 5 },
    { utf16: 4, utf8: 6 },
    { utf16: 5, utf8: 8 },
    { utf16: 6, utf8: 11 },
  ]);
  assert.equal(index.graphemeCount, 4);
  assert.deepEqual(
    index.boundaries.map((at) => at.utf16),
    [0, 1, 3, 5, 6],
  );
  for (const at of index.positions) {
    assert.deepEqual(index.positionAt(at.utf16, "utf16"), at);
    assert.deepEqual(index.positionAt(at.utf8, "utf8"), at);
    assert.equal(
      new TextEncoder().encode(index.text.slice(0, at.utf16)).length,
      at.utf8,
    );
  }
});

test("previous/next snap strictly outward from inside a grapheme", () => {
  const index = new UnicodeText("A👩🏽‍🚀e\u0301");
  const interior = index.positionAt(5, "utf16");
  assert.deepEqual(index.move(interior, "previous"), { utf16: 1, utf8: 1 });
  assert.deepEqual(index.move(interior, "next"), { utf16: 8, utf8: 16 });
  const combining = index.positionAt(9, "utf16");
  assert.deepEqual(index.move(combining, "previous"), { utf16: 8, utf8: 16 });
  assert.deepEqual(index.move(combining, "next"), { utf16: 10, utf8: 19 });
});

test("empty text has one index position and an end-of-text line break", () => {
  const analysis = analyzeText("");
  assert.equal(analysis.unicodeVersion, UNICODE_VERSION);
  assert.equal(analysis.graphemeCount, 0);
  assert.deepEqual(analysis.graphemes, []);
  assert.deepEqual(analysis.cursor, [
    { at: { utf16: 0, utf8: 0 }, previous: null, next: null },
  ]);
  assert.deepEqual(analysis.lineBreaks, [
    { at: { utf16: 0, utf8: 0 }, required: true },
  ]);
});

test("cursor ends return null rather than clamping and looping", () => {
  const index = new UnicodeText("😀x");
  assert.equal(index.move(index.positionAt(0, "utf16"), "previous"), null);
  assert.equal(index.move(index.length, "next"), null);
  assert.deepEqual(index.move(index.length, "previous"), { utf16: 2, utf8: 4 });
});

test("splitting a surrogate or UTF-8 scalar is an error", () => {
  const index = new UnicodeText("A😀中");
  assert.throws(
    () => index.positionAt(2, "utf16"),
    hasCode("invalid_position"),
  );
  for (const byte of [2, 3, 4, 6, 7]) {
    assert.throws(
      () => index.positionAt(byte, "utf8"),
      hasCode("invalid_position"),
    );
  }
});

test("mismatched paired positions and invalid direction are rejected", () => {
  const index = new UnicodeText("中");
  assert.throws(
    () => index.move({ utf16: 1, utf8: 1 }, "next"),
    hasCode("invalid_position"),
  );
  assert.throws(() => index.move(null, "next"), hasCode("invalid_position"));
  assert.throws(
    () => index.move({ utf16: 0, utf8: 0 }, "left"),
    hasCode("invalid_direction"),
  );
});

test("numeric offsets must be finite nonnegative safe integers", () => {
  const index = new UnicodeText("X");
  for (const value of [
    -1,
    0.5,
    NaN,
    Infinity,
    2,
    Number.MAX_SAFE_INTEGER + 1,
    "0",
  ]) {
    assert.throws(
      () => index.positionAt(value, "utf16"),
      hasCode("invalid_position"),
    );
  }
  assert.throws(
    () => index.positionAt(0, "codepoints"),
    hasCode("invalid_position"),
  );
});

test("lone surrogates are rejected instead of replaced with U+FFFD", () => {
  for (const text of [
    "\ud800",
    "\udc00",
    "\ud800X",
    "X\udc00",
    "\ud800\ud800",
  ]) {
    assert.throws(() => new UnicodeText(text), hasCode("invalid_text"));
  }
  assert.throws(() => scalarPositions(null), hasCode("invalid_text"));
  assert.equal(new UnicodeText("\ufffd").length.utf8, 3);
  assert.equal(new UnicodeText("\ud800\udc00").length.utf8, 4);
});

test("text limit is enforced before Unicode algorithms run", () => {
  assert.throws(
    () => new UnicodeText("A".repeat(MAX_TEXT_UTF16 + 1)),
    hasCode("limit_exceeded"),
  );
  assert.equal(
    scalarPositions("A".repeat(MAX_TEXT_UTF16)).at(-1).utf16,
    MAX_TEXT_UTF16,
  );
});

test("every scalar offset matches native UTF-8 encoding across byte ranges", () => {
  const text =
    "\0\u007f\u0080\u07ff\u0800\ud7ff\ue000\uffff\u{10000}\u{10ffff}";
  for (const at of scalarPositions(text)) {
    assert.equal(at.utf8, Buffer.byteLength(text.slice(0, at.utf16), "utf8"));
  }
});

test("all returned arrays and paired positions are immutable", () => {
  const index = new UnicodeText("A😀");
  assert.throws(() => index.positions.push({ utf16: 9, utf8: 9 }), TypeError);
  assert.throws(() => {
    index.positions[1].utf8 = 8;
  }, TypeError);
  assert.throws(() => {
    index.graphemes[0].text = "B";
  }, TypeError);
  assert.throws(() => {
    index.text = "other";
  }, TypeError);
  assert.equal(index.lineBreaks(), index.lineBreaks());
  assert.throws(() => index.lineBreaks().pop(), TypeError);
});

test("normalization, soft hyphens and embedded BOMs are not silently changed", () => {
  const composed = new UnicodeText("\u00e9");
  const decomposed = new UnicodeText("e\u0301");
  assert.equal(composed.graphemeCount, decomposed.graphemeCount);
  assert.notDeepEqual(composed.length, decomposed.length);
  const text = "по\u00adлеA\ufeffB";
  assert.equal(
    new UnicodeText(text).graphemes.map((part) => part.text).join(""),
    text,
  );
});

test("normative LB8a forbids breaks inside family and profession ZWJ emoji", () => {
  for (const text of ["👨‍👩‍👧‍👦", "👩🏽‍🚀", "👨‍💻", "😀\u200d😀"]) {
    assert.deepEqual(new UnicodeText(text).lineBreaks(), [
      {
        at: { utf16: text.length, utf8: Buffer.byteLength(text) },
        required: true,
      },
    ]);
  }
  assert.deepEqual(
    new UnicodeText("👩🏽‍🚀👨‍💻").lineBreaks().map((item) => item.at.utf16),
    [7, 12],
  );
});

test("ZWJ at start, after spaces, and at end keeps LB8a / LB3 rule order", () => {
  for (const text of ["\u200d😀", " \u200d😀", "😀\u200d"]) {
    const breaks = new UnicodeText(text).lineBreaks();
    assert.ok(
      breaks.every(
        (item) =>
          item.at.utf16 === text.length ||
          text.charCodeAt(item.at.utf16 - 1) !== 0x200d,
      ),
    );
    assert.equal(breaks.at(-1).required, true);
  }
});

test("UAX #14 scalar opportunities are not silently intersected with UAX #29", () => {
  const index = new UnicodeText(" \u0301A");
  assert.deepEqual(
    index.boundaries.map((at) => at.utf16),
    [0, 2, 3],
  );
  assert.deepEqual(
    index.lineBreaks().map((item) => item.at.utf16),
    [1, 3],
  );
});

test("mandatory breaks retain CRLF and all source characters", () => {
  const index = new UnicodeText("A\r\nB\n");
  assert.deepEqual(index.lineBreaks(), [
    { at: { utf16: 3, utf8: 3 }, required: true },
    { at: { utf16: 5, utf8: 5 }, required: true },
  ]);
  assert.deepEqual(
    index.graphemes.map((part) => part.text),
    ["A", "\r\n", "B", "\n"],
  );
});

test("stable result is independent of the machine locale", () => {
  const text = "क्षि א\u05b0 中 👩🏽‍🚀";
  const first = analyzeText(text);
  assert.deepEqual(analyzeText(text), first);
  const old = process.env.LANG;
  try {
    process.env.LANG = "ar";
    assert.deepEqual(analyzeText(text), first);
  } finally {
    if (old === undefined) delete process.env.LANG;
    else process.env.LANG = old;
  }
});

test("native Unicode version drift is explicitly rejected by the UAX #14 adapter", () => {
  const descriptor = Object.getOwnPropertyDescriptor(
    process.versions,
    "unicode",
  );
  try {
    Object.defineProperty(process.versions, "unicode", { value: "16.0" });
    const index = new UnicodeText("क्षि");
    assert.equal(index.graphemeCount, 1);
    assert.throws(() => index.lineBreaks(), hasCode("unsupported_runtime"));
  } finally {
    Object.defineProperty(process.versions, "unicode", descriptor);
  }
  assert.equal(new UnicodeText("A").lineBreaks().at(-1).required, true);
});
