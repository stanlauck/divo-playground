// SPDX-License-Identifier: MIT OR Apache-2.0

import {
  CORPUS_VERSION,
  MAX_CORPUS_CASES,
  MAX_CORPUS_JSON_UTF16,
  MAX_CORPUS_TEXT_UTF16,
  MAX_TEXT_UTF16,
  UNICODE_ERROR_CODES,
  UNICODE_VERSION,
  UnicodeError,
  type Analyzer,
  type CheckIssue,
  type CheckReport,
  type Corpus,
  type CorpusCase,
  type CursorStep,
  type DeferredFeature,
  type Expectations,
  type Grapheme,
  type LineBreak,
  type Position,
} from "./model.js";
import { analyzeText, scalarPositions } from "./unicode.js";

function invalid(message: string): never {
  throw new UnicodeError("invalid_corpus", message);
}

function object(
  value: unknown,
  fields: readonly string[],
  path: string,
): Record<string, unknown> {
  if (
    value === null ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    (Object.getPrototypeOf(value) !== Object.prototype &&
      Object.getPrototypeOf(value) !== null)
  ) {
    return invalid(`${path} must be an object`);
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !fields.includes(key))) {
    return invalid(`${path} contains an unknown field`);
  }
  return record;
}

function list(value: unknown, maximum: number, path: string): unknown[] {
  if (!Array.isArray(value)) {
    return invalid(`${path} must be an array`);
  }
  if (value.length > maximum) {
    throw new UnicodeError("limit_exceeded", `${path} exceeds its item limit`);
  }
  for (let i = 0; i < value.length; i += 1) {
    if (!Object.hasOwn(value, i)) {
      return invalid(`${path} must not contain missing array items`);
    }
  }
  return value;
}

function integer(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    return invalid(`${path} must be a nonnegative safe integer`);
  }
  return value;
}

function token(value: unknown, path: string): string {
  if (typeof value !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/.test(value)) {
    return invalid(`${path} must be a bounded ASCII identifier`);
  }
  return value;
}

function unique(values: readonly string[], path: string): void {
  if (new Set(values).size !== values.length) {
    invalid(`${path} contains a duplicate`);
  }
}

function position(
  value: unknown,
  offsets: ReadonlyMap<number, number>,
  path: string,
): Position {
  const record = object(value, ["utf16", "utf8"], path);
  const utf16 = integer(record.utf16, `${path}.utf16`);
  const utf8 = integer(record.utf8, `${path}.utf8`);
  if (offsets.get(utf16) !== utf8) {
    return invalid(
      `${path} does not identify the same scalar in both encodings`,
    );
  }
  return Object.freeze({ utf16, utf8 });
}

function nullablePosition(
  value: unknown,
  offsets: ReadonlyMap<number, number>,
  path: string,
): Position | null {
  return value === null ? null : position(value, offsets, path);
}

function expectations(
  value: unknown,
  text: string,
  positions: readonly Position[],
  path: string,
): Expectations {
  const record = object(
    value,
    ["graphemeCount", "graphemes", "cursor", "lineBreaks"],
    path,
  );
  const offsets = new Map(positions.map((at) => [at.utf16, at.utf8]));
  const count = integer(record.graphemeCount, `${path}.graphemeCount`);
  const graphemes: Grapheme[] = [];
  let last = 0;
  for (const [i, value] of list(
    record.graphemes,
    text.length,
    `${path}.graphemes`,
  ).entries()) {
    const partPath = `${path}.graphemes[${i}]`;
    const part = object(value, ["text", "start", "end"], partPath);
    const start = position(part.start, offsets, `${partPath}.start`);
    const end = position(part.end, offsets, `${partPath}.end`);
    if (
      start.utf16 !== last ||
      end.utf16 <= start.utf16 ||
      typeof part.text !== "string" ||
      part.text !== text.slice(start.utf16, end.utf16)
    ) {
      return invalid(
        `${partPath} must preserve a contiguous original text slice`,
      );
    }
    graphemes.push(Object.freeze({ text: part.text, start, end }));
    last = end.utf16;
  }
  if (last !== text.length || count !== graphemes.length) {
    return invalid(
      `${path}.graphemes must partition the text and match its count`,
    );
  }

  const cursor: CursorStep[] = [];
  const probes = list(record.cursor, positions.length, `${path}.cursor`);
  if (probes.length !== positions.length) {
    return invalid(`${path}.cursor must probe every scalar boundary`);
  }
  for (const [i, value] of probes.entries()) {
    const probePath = `${path}.cursor[${i}]`;
    const probe = object(value, ["at", "previous", "next"], probePath);
    const at = position(probe.at, offsets, `${probePath}.at`);
    if (at.utf16 !== positions[i]!.utf16) {
      return invalid(`${probePath}.at is not the next scalar boundary`);
    }
    const previous = nullablePosition(
      probe.previous,
      offsets,
      `${probePath}.previous`,
    );
    const next = nullablePosition(probe.next, offsets, `${probePath}.next`);
    if (
      (previous !== null && previous.utf16 >= at.utf16) ||
      (next !== null && next.utf16 <= at.utf16)
    ) {
      return invalid(`${probePath} must move strictly in logical text order`);
    }
    cursor.push(Object.freeze({ at, previous, next }));
  }

  const lineBreaks: LineBreak[] = [];
  let previousOffset = -1;
  for (const [i, value] of list(
    record.lineBreaks,
    positions.length,
    `${path}.lineBreaks`,
  ).entries()) {
    const breakPath = `${path}.lineBreaks[${i}]`;
    const item = object(value, ["at", "required"], breakPath);
    const at = position(item.at, offsets, `${breakPath}.at`);
    if (
      typeof item.required !== "boolean" ||
      at.utf16 <= previousOffset ||
      (at.utf16 === 0 && text.length !== 0)
    ) {
      return invalid(
        `${breakPath} must be an ordered break with a boolean flag`,
      );
    }
    lineBreaks.push(Object.freeze({ at, required: item.required }));
    previousOffset = at.utf16;
  }
  const end = lineBreaks[lineBreaks.length - 1];
  if (end?.at.utf16 !== text.length || !end.required) {
    return invalid(
      `${path}.lineBreaks must end in a required end-of-text break`,
    );
  }
  return Object.freeze({
    graphemeCount: count,
    graphemes: Object.freeze(graphemes),
    cursor: Object.freeze(cursor),
    lineBreaks: Object.freeze(lineBreaks),
  });
}

/** Structural/index validation only: does not derive the Unicode oracle. */
export function validateCorpus(value: unknown): Corpus {
  const root = object(
    value,
    [
      "version",
      "unicodeVersion",
      "indexEncodings",
      "cursorMode",
      "lineBreakMode",
      "normalization",
      "cases",
    ],
    "corpus",
  );
  const encodings = list(root.indexEncodings, 2, "corpus.indexEncodings");
  if (
    root.version !== CORPUS_VERSION ||
    root.unicodeVersion !== UNICODE_VERSION ||
    encodings.length !== 2 ||
    encodings[0] !== "utf16" ||
    encodings[1] !== "utf8" ||
    root.cursorMode !== "logical" ||
    root.lineBreakMode !== "uax14-default" ||
    root.normalization !== "none"
  ) {
    return invalid("Unsupported corpus version or Unicode/indexing profile");
  }
  const cases: CorpusCase[] = [];
  let totalLength = 0;
  for (const [i, value] of list(
    root.cases,
    MAX_CORPUS_CASES,
    "corpus.cases",
  ).entries()) {
    const path = `corpus.cases[${i}]`;
    const item = object(
      value,
      ["id", "tags", "text", "deferred", "expected"],
      path,
    );
    const id = token(item.id, `${path}.id`);
    const tags = list(item.tags, 16, `${path}.tags`).map((tag) =>
      token(tag, `${path}.tags`),
    );
    if (tags.length === 0) {
      return invalid(`${path}.tags must not be empty`);
    }
    unique(tags, `${path}.tags`);
    if (typeof item.text !== "string") {
      return invalid(`${path}.text must be a string`);
    }
    const positions = scalarPositions(item.text);
    totalLength += item.text.length;
    if (totalLength > MAX_CORPUS_TEXT_UTF16) {
      throw new UnicodeError(
        "limit_exceeded",
        "Corpus exceeds the total text limit",
      );
    }
    const expected = expectations(
      item.expected,
      item.text,
      positions,
      `${path}.expected`,
    );
    let deferred: readonly DeferredFeature[] | undefined;
    if (Object.hasOwn(item, "deferred")) {
      deferred = Object.freeze(
        list(item.deferred, 2, `${path}.deferred`).map((feature) => {
          if (feature !== "visual_cursor" && feature !== "bidi_reordering") {
            return invalid(`${path}.deferred contains an unknown feature`);
          }
          return feature;
        }),
      );
      unique(deferred, `${path}.deferred`);
    }
    cases.push(
      Object.freeze({
        id,
        tags: Object.freeze(tags),
        text: item.text,
        ...(deferred === undefined ? {} : { deferred }),
        expected,
      }),
    );
  }
  if (cases.length === 0) {
    return invalid("Corpus must contain at least one case");
  }
  unique(
    cases.map((item) => item.id),
    "corpus.cases ids",
  );
  return Object.freeze({
    version: CORPUS_VERSION,
    unicodeVersion: UNICODE_VERSION,
    indexEncodings: Object.freeze(["utf16", "utf8"] as const),
    cursorMode: "logical",
    lineBreakMode: "uax14-default",
    normalization: "none",
    cases: Object.freeze(cases),
  });
}

export function parseCorpus(json: string): Corpus {
  if (typeof json !== "string") {
    return invalid("Corpus JSON must be a string");
  }
  if (json.length > MAX_CORPUS_JSON_UTF16) {
    throw new UnicodeError(
      "limit_exceeded",
      "Corpus JSON exceeds its length limit",
    );
  }
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    return invalid("Corpus JSON is malformed");
  }
  return validateCorpus(value);
}

function same(expected: unknown, actual: unknown): boolean {
  if (expected === actual) {
    return true;
  }
  if (
    expected === null ||
    actual === null ||
    typeof expected !== "object" ||
    typeof actual !== "object"
  ) {
    return false;
  }
  if (Array.isArray(expected) || Array.isArray(actual)) {
    return (
      Array.isArray(expected) &&
      Array.isArray(actual) &&
      expected.length === actual.length &&
      expected.every((value, i) => same(value, actual[i]))
    );
  }
  const a = expected as Record<string, unknown>;
  const b = actual as Record<string, unknown>;
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length &&
    keys.every((key) => Object.hasOwn(b, key) && same(a[key], b[key]))
  );
}

function copyJson(value: unknown, active: Set<object>, depth: number): unknown {
  if (depth > 32) throw new Error("Snapshot depth limit");
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "boolean"
  ) {
    return value;
  }
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value !== "object" || active.has(value)) {
    throw new Error("Not plain JSON data");
  }
  active.add(value);
  try {
    const keys = Reflect.ownKeys(value);
    if (Array.isArray(value)) {
      if (
        Object.getPrototypeOf(value) !== Array.prototype ||
        value.length > MAX_TEXT_UTF16 + 1 ||
        keys.length !== value.length + 1
      ) {
        throw new Error("Not a bounded plain JSON array");
      }
      const result: unknown[] = [];
      for (let i = 0; i < value.length; i += 1) {
        const descriptor = Object.getOwnPropertyDescriptor(value, i);
        if (!descriptor?.enumerable || !Object.hasOwn(descriptor, "value")) {
          throw new Error("Sparse or accessor array");
        }
        result.push(copyJson(descriptor.value, active, depth + 1));
      }
      return Object.freeze(result);
    }
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== null && prototype !== Object.prototype) {
      throw new Error("Not a plain JSON object");
    }
    const result: Record<string, unknown> = {};
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (
        typeof key !== "string" ||
        key === "toJSON" ||
        !descriptor?.enumerable ||
        !Object.hasOwn(descriptor, "value")
      ) {
        throw new Error("Not plain JSON object data");
      }
      Object.defineProperty(result, key, {
        value: copyJson(descriptor.value, active, depth + 1),
        enumerable: true,
      });
    }
    return Object.freeze(result);
  } finally {
    active.delete(value);
  }
}

function snapshot(value: unknown): unknown {
  try {
    return copyJson(value, new Set(), 0);
  } catch {
    return "non-JSON analyzer value";
  }
}

/** A custom analyzer can check another backend's portable result shape. */
export function checkCorpus(
  corpus: Corpus,
  analyzer: Analyzer = analyzeText,
): CheckReport {
  const input = validateCorpus(corpus);
  const issues: CheckIssue[] = [];
  let failed = 0;
  for (const item of input.cases) {
    const countBefore = issues.length;
    try {
      const actual = analyzer(item.text);
      if (actual?.unicodeVersion !== input.unicodeVersion) {
        issues.push(
          Object.freeze({
            caseId: item.id,
            field: "unicodeVersion",
            expected: input.unicodeVersion,
            actual: snapshot(actual?.unicodeVersion ?? null),
          }),
        );
      }
      for (const field of [
        "graphemeCount",
        "graphemes",
        "cursor",
        "lineBreaks",
      ] as const) {
        if (!same(item.expected[field], actual?.[field])) {
          issues.push(
            Object.freeze({
              caseId: item.id,
              field,
              expected: item.expected[field],
              actual: snapshot(actual?.[field] ?? null),
            }),
          );
        }
      }
    } catch (error: unknown) {
      // Keep only allowlisted package codes, never exception messages or paths.
      const diagnostic =
        error instanceof UnicodeError &&
        UNICODE_ERROR_CODES.includes(error.code)
          ? Object.freeze({ errorCode: error.code })
          : "analyzer failed";
      issues.push(
        Object.freeze({
          caseId: item.id,
          field: "analyzer",
          expected: "successful analysis",
          actual: diagnostic,
        }),
      );
    }
    if (issues.length !== countBefore) {
      failed += 1;
    }
  }
  return Object.freeze({
    version: CORPUS_VERSION,
    unicodeVersion: UNICODE_VERSION,
    caseCount: input.cases.length,
    passed: input.cases.length - failed,
    failed,
    issues: Object.freeze(issues),
  });
}
