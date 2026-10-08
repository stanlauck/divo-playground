// SPDX-License-Identifier: MIT OR Apache-2.0

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  MAX_CORPUS_CASES,
  MAX_CORPUS_JSON_UTF16,
  MAX_CORPUS_TEXT_UTF16,
  UnicodeError,
  UnicodeText,
  analyzeText,
  checkCorpus,
  parseCorpus,
  validateCorpus,
} from "../dist/src/index.js";
import { definitions, makeCorpus } from "../examples/support/fixtures.mjs";

const json = await readFile(
  new URL("../corpus/corpus.json", import.meta.url),
  "utf8",
);
const corpus = parseCorpus(json);
const hasCode = (code) => (error) =>
  error instanceof UnicodeError && error.code === code;
const changed = (modify) => {
  const copy = JSON.parse(json);
  modify(copy);
  return copy;
};
const one = (id) =>
  validateCorpus({
    ...corpus,
    cases: [corpus.cases.find((item) => item.id === id)],
  });

for (const item of corpus.cases) {
  test(`independent corpus oracle: ${item.id}`, () => {
    const actual = analyzeText(item.text);
    assert.deepEqual(
      {
        graphemeCount: actual.graphemeCount,
        graphemes: actual.graphemes,
        cursor: actual.cursor,
        lineBreaks: actual.lineBreaks,
      },
      item.expected,
    );
    const index = new UnicodeText(item.text);
    for (const probe of item.expected.cursor) {
      assert.equal(
        probe.at.utf8,
        Buffer.byteLength(item.text.slice(0, probe.at.utf16)),
      );
      assert.deepEqual(index.positionAt(probe.at.utf8, "utf8"), probe.at);
    }
    const forward = [];
    let at = index.positionAt(0, "utf16");
    while (at !== null) {
      forward.push(at);
      at = index.move(at, "next");
    }
    assert.deepEqual(forward, index.boundaries);
    const backward = [];
    at = index.length;
    while (at !== null) {
      backward.push(at);
      at = index.move(at, "previous");
    }
    assert.deepEqual(backward.reverse(), index.boundaries);
  });
}

test("committed corpus reproduces the independent definitions", () => {
  assert.equal(JSON.stringify(makeCorpus(), null, 2) + "\n", json);
  assert.equal(definitions.length, 44);
});

test("all corpus checks pass with no missing or silently skipped cases", () => {
  assert.deepEqual(checkCorpus(corpus), {
    version: 1,
    unicodeVersion: "17.0.0",
    caseCount: 44,
    passed: 44,
    failed: 0,
    issues: [],
  });
});

test("two RTL cases explicitly defer visual cursor and BiDi only", () => {
  const rtl = corpus.cases.filter((item) => item.tags.includes("rtl"));
  assert.equal(rtl.length, 2);
  for (const item of rtl) {
    assert.deepEqual(item.deferred, ["visual_cursor", "bidi_reordering"]);
    assert.equal(checkCorpus(one(item.id)).passed, 1);
  }
});

test("required characters and scripts really occur in the original corpus text", () => {
  const text = corpus.cases.map((item) => item.text).join("");
  for (const character of [
    "\u00a0",
    "\u202f",
    "\u00ad",
    "-",
    "—",
    "–",
    "👩🏽‍🚀",
    "क्षि",
    "中",
    "א",
    "ا",
  ]) {
    assert.ok(text.includes(character));
  }
  assert.equal(one("nbsp").cases[0].expected.lineBreaks.length, 1);
  assert.equal(one("narrow-nbsp").cases[0].expected.lineBreaks.length, 1);
  assert.equal(one("soft-hyphen").cases[0].expected.lineBreaks[0].at.utf16, 3);
});

test("cross-language byte-offset corruption is reported independently of cluster text", () => {
  const report = checkCorpus(one("emoji-modifier"), (text) => {
    const actual = analyzeText(text);
    return {
      ...actual,
      graphemes: actual.graphemes.map((part) => ({
        ...part,
        end: { utf16: part.end.utf16, utf8: part.end.utf16 },
      })),
    };
  });
  assert.equal(report.failed, 1);
  assert.deepEqual(
    report.issues.map((issue) => issue.field),
    ["graphemes"],
  );
});

test("cursor and line-break mutations have separate report fields", () => {
  const report = checkCorpus(one("soft-hyphen"), (text) => {
    const actual = analyzeText(text);
    return { ...actual, cursor: [], lineBreaks: [] };
  });
  assert.equal(report.failed, 1);
  assert.deepEqual(
    report.issues.map((issue) => issue.field),
    ["cursor", "lineBreaks"],
  );
});

test("custom result object property order does not affect equality", () => {
  assert.equal(
    checkCorpus(corpus, (text) =>
      JSON.parse(
        JSON.stringify(analyzeText(text), (key, value) => {
          if (value && !Array.isArray(value) && typeof value === "object") {
            return Object.fromEntries(Object.entries(value).reverse());
          }
          return value;
        }),
      ),
    ).failed,
    0,
  );
});

test("wrong Unicode version is a failed check, not an implicit upgrade", () => {
  const report = checkCorpus(one("empty"), (text) => ({
    ...analyzeText(text),
    unicodeVersion: "16.0.0",
  }));
  assert.equal(report.failed, 1);
  assert.equal(report.issues[0].field, "unicodeVersion");
});

test("analyzer exceptions are sanitized and other cases still run", () => {
  const report = checkCorpus(corpus, (text) => {
    if (text === "") throw new Error("private backend diagnostic");
    return analyzeText(text);
  });
  assert.equal(report.passed, 43);
  assert.equal(report.failed, 1);
  assert.deepEqual(report.issues[0], {
    caseId: "empty",
    field: "analyzer",
    expected: "successful analysis",
    actual: "analyzer failed",
  });
  assert.ok(!JSON.stringify(report).includes("private backend diagnostic"));
});

test("reports snapshot reused backend arrays, including nested paired positions", () => {
  const input = validateCorpus({ ...corpus, cases: corpus.cases.slice(0, 2) });
  const reused = [];
  const report = checkCorpus(input, (text) => {
    const actual = analyzeText(text);
    reused.splice(0, reused.length, ...structuredClone(actual.cursor));
    reused[0].at.utf8 = text.length + 99;
    return { ...actual, cursor: reused };
  });
  const first = report.issues.find((issue) => issue.caseId === "empty");
  const second = report.issues.find((issue) => issue.caseId === "ascii-space");
  assert.equal(first.actual[0].at.utf8, 99);
  assert.equal(second.actual[0].at.utf8, 102);
  reused[0].at.utf8 = 500;
  reused.length = 0;
  assert.equal(first.actual[0].at.utf8, 99);
  assert.equal(second.actual[0].at.utf8, 102);
  assert.throws(() => {
    first.actual[0].at.utf8 = 9;
  }, TypeError);
  assert.ok(Object.isFrozen(first.actual));
  assert.equal(
    JSON.parse(JSON.stringify(report)).issues[0].actual[0].at.utf8,
    99,
  );
});

test("non-JSON analyzer mismatch values are replaced with a stable diagnostic", () => {
  const cyclic = [];
  cyclic.push(cyclic);
  for (const value of [cyclic, [undefined], [1n], [NaN], [() => {}]]) {
    const report = checkCorpus(one("empty"), (text) => ({
      ...analyzeText(text),
      cursor: value,
    }));
    assert.equal(report.failed, 1);
    assert.equal(report.issues[0].actual, "non-JSON analyzer value");
    assert.doesNotThrow(() => JSON.stringify(report));
  }
});

test("known package error codes remain actionable without disclosing messages", () => {
  for (const code of ["unsupported_runtime", "backend_invariant"]) {
    const report = checkCorpus(one("empty"), () => {
      throw new UnicodeError(code, "private backend diagnostic");
    });
    assert.deepEqual(report.issues[0].actual, { errorCode: code });
    assert.ok(!JSON.stringify(report).includes("private backend diagnostic"));
  }
  const unknown = checkCorpus(one("empty"), () => {
    throw new UnicodeError(
      "private-backend-code",
      "private backend diagnostic",
    );
  });
  assert.equal(unknown.issues[0].actual, "analyzer failed");
  assert.ok(!JSON.stringify(unknown).includes("private-backend-code"));
});

test("default checker reports unsupported native Unicode versions with a safe code", () => {
  const descriptor = Object.getOwnPropertyDescriptor(
    process.versions,
    "unicode",
  );
  try {
    Object.defineProperty(process.versions, "unicode", { value: "18.0" });
    const report = checkCorpus(one("empty"));
    assert.equal(report.failed, 1);
    assert.deepEqual(report.issues[0].actual, {
      errorCode: "unsupported_runtime",
    });
  } finally {
    Object.defineProperty(process.versions, "unicode", descriptor);
  }
});

test("JSON syntax, profile drift and unknown fields are rejected", () => {
  assert.throws(() => parseCorpus("{"), hasCode("invalid_corpus"));
  assert.throws(() => parseCorpus(null), hasCode("invalid_corpus"));
  for (const modify of [
    (value) => {
      value.version = 2;
    },
    (value) => {
      value.unicodeVersion = "16.0.0";
    },
    (value) => {
      value.cursorMode = "visual";
    },
    (value) => {
      value.lineBreakMode = "wrap-columns";
    },
    (value) => {
      value.normalization = "nfc";
    },
    (value) => {
      value.indexEncodings.reverse();
    },
    (value) => {
      value.unknown = true;
    },
    (value) => {
      value.cases[0].extra = true;
    },
  ]) {
    assert.throws(
      () => validateCorpus(changed(modify)),
      hasCode("invalid_corpus"),
    );
  }
});

test("unknown nested fields and prototype-shaped inputs are not accepted", () => {
  for (const modify of [
    (value) => {
      value.cases[1].expected.extra = true;
    },
    (value) => {
      value.cases[1].expected.graphemes[0].start.extra = true;
    },
    (value) => {
      value.cases[1].expected.cursor[0].extra = true;
    },
    (value) => {
      value.cases[1].expected.lineBreaks[0].extra = true;
    },
  ]) {
    assert.throws(
      () => validateCorpus(changed(modify)),
      hasCode("invalid_corpus"),
    );
  }
  assert.throws(
    () => validateCorpus(Object.create(corpus)),
    hasCode("invalid_corpus"),
  );
  const polluted = JSON.parse(
    json.slice(0, -2) + ',"__proto__":{"polluted":true}}',
  );
  assert.throws(() => validateCorpus(polluted), hasCode("invalid_corpus"));
  assert.equal({}.polluted, undefined);
});

test("invalid offsets, scalar splits and incomplete cursor coverage are rejected", () => {
  for (const modify of [
    (value) => {
      value.cases[1].expected.graphemes[0].end.utf8 = 2;
    },
    (value) => {
      value.cases[1].expected.cursor.pop();
    },
    (value) => {
      value.cases[1].expected.cursor[0].at.utf16 = 0.5;
    },
    (value) => {
      value.cases[1].expected.cursor[0].at.utf8 = -1;
    },
    (value) => {
      value.cases[1].expected.lineBreaks[0].required = "false";
    },
  ]) {
    assert.throws(
      () => validateCorpus(changed(modify)),
      hasCode("invalid_corpus"),
    );
  }
  assert.throws(
    () =>
      validateCorpus(
        changed((value) => {
          const emoji = value.cases.find(
            (item) => item.id === "emoji-supplementary",
          );
          emoji.expected.graphemes[0].end = { utf16: 1, utf8: 1 };
        }),
      ),
    hasCode("invalid_corpus"),
  );
});

test("expectations must partition the unchanged original text", () => {
  for (const modify of [
    (value) => {
      value.cases[1].expected.graphemes[0].text = "changed";
    },
    (value) => {
      value.cases[1].expected.graphemeCount = 0;
    },
    (value) => {
      value.cases[1].expected.graphemes.pop();
    },
    (value) => {
      value.cases[1].expected.lineBreaks.pop();
    },
  ]) {
    assert.throws(
      () => validateCorpus(changed(modify)),
      hasCode("invalid_corpus"),
    );
  }
});

test("duplicate ids/tags, missing fields and unsupported deferred features are rejected", () => {
  for (const modify of [
    (value) => {
      value.cases[1].id = value.cases[0].id;
    },
    (value) => {
      value.cases[0].tags.push(value.cases[0].tags[0]);
    },
    (value) => {
      value.cases[0].tags = [];
    },
    (value) => {
      value.cases[0].deferred = ["unknown"];
    },
    (value) => {
      delete value.cases[0].text;
    },
    (value) => {
      value.cases = [];
    },
  ]) {
    assert.throws(
      () => validateCorpus(changed(modify)),
      hasCode("invalid_corpus"),
    );
  }
});

test("corpus limits reject oversized JSON and too many cases", () => {
  assert.throws(
    () => parseCorpus(" ".repeat(MAX_CORPUS_JSON_UTF16 + 1)),
    hasCode("limit_exceeded"),
  );
  assert.throws(
    () =>
      validateCorpus({
        ...corpus,
        cases: Array(MAX_CORPUS_CASES + 1).fill(corpus.cases[0]),
      }),
    hasCode("limit_exceeded"),
  );
});

test("validated corpus is detached from mutable caller data", () => {
  const mutable = JSON.parse(json);
  const validated = validateCorpus(mutable);
  mutable.cases[1].expected.graphemes[0].start.utf8 = 99;
  mutable.cases[1].tags.push("changed");
  assert.equal(validated.cases[1].expected.graphemes[0].start.utf8, 0);
  assert.ok(!validated.cases[1].tags.includes("changed"));
  assert.throws(() => validated.cases.pop(), TypeError);
});

test("programmatic sparse arrays cannot bypass strict corpus validation", () => {
  for (const modify of [
    (value) => {
      value.cases[0].tags = Array(1);
    },
    (value) => {
      value.cases[0].deferred = Array(1);
    },
    (value) => {
      delete value.indexEncodings[1];
    },
  ]) {
    assert.throws(
      () => validateCorpus(changed(modify)),
      hasCode("invalid_corpus"),
    );
  }
});

test("large corpus aggregate text limit is checked before result collection", () => {
  const text = "A".repeat(100_000);
  const positions = Array.from({ length: text.length + 1 }, (_, offset) => ({
    utf16: offset,
    utf8: offset,
  }));
  const expected = {
    graphemeCount: 1,
    graphemes: [{ text, start: positions[0], end: positions.at(-1) }],
    cursor: positions.map((at) => ({
      at,
      previous: at.utf16 === 0 ? null : positions[0],
      next: at.utf16 === text.length ? null : positions.at(-1),
    })),
    lineBreaks: [{ at: positions.at(-1), required: true }],
  };
  assert.throws(
    () =>
      validateCorpus({
        ...corpus,
        cases: Array.from(
          { length: MAX_CORPUS_TEXT_UTF16 / text.length + 1 },
          (_, i) => ({
            id: `large-${i}`,
            tags: ["limit"],
            text,
            expected,
          }),
        ),
      }),
    hasCode("limit_exceeded"),
  );
});
