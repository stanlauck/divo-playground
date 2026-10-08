// SPDX-License-Identifier: MIT OR Apache-2.0

export const UNICODE_VERSION = "17.0.0" as const;
export const CORPUS_VERSION = 1 as const;
export const MAX_TEXT_UTF16 = 100_000;
export const MAX_CORPUS_TEXT_UTF16 = 1_000_000;
export const MAX_CORPUS_CASES = 256;
export const MAX_CORPUS_JSON_UTF16 = 16_777_216;

export type Encoding = "utf16" | "utf8";
export type CursorDirection = "previous" | "next";
export type DeferredFeature = "visual_cursor" | "bidi_reordering";

/** Offsets from the beginning of the original, unnormalized text. */
export interface Position {
  readonly utf16: number;
  readonly utf8: number;
}

export interface Grapheme {
  readonly text: string;
  readonly start: Position;
  readonly end: Position;
}

export interface CursorStep {
  readonly at: Position;
  readonly previous: Position | null;
  readonly next: Position | null;
}

export interface LineBreak {
  readonly at: Position;
  readonly required: boolean;
}

export interface Expectations {
  readonly graphemeCount: number;
  readonly graphemes: readonly Grapheme[];
  /** One probe at every Unicode scalar boundary, including start and end. */
  readonly cursor: readonly CursorStep[];
  readonly lineBreaks: readonly LineBreak[];
}

export interface CorpusCase {
  readonly id: string;
  readonly tags: readonly string[];
  readonly text: string;
  readonly deferred?: readonly DeferredFeature[];
  readonly expected: Expectations;
}

export interface Corpus {
  readonly version: typeof CORPUS_VERSION;
  readonly unicodeVersion: typeof UNICODE_VERSION;
  readonly indexEncodings: readonly ["utf16", "utf8"];
  readonly cursorMode: "logical";
  readonly lineBreakMode: "uax14-default";
  readonly normalization: "none";
  readonly cases: readonly CorpusCase[];
}

/** Portable JSON shape that another implementation, including Rust, can emit. */
export interface TextAnalysis extends Expectations {
  readonly unicodeVersion: string;
}

export type Analyzer = (text: string) => TextAnalysis;

export interface CheckIssue {
  readonly caseId: string;
  readonly field: keyof Expectations | "unicodeVersion" | "analyzer";
  readonly expected: unknown;
  readonly actual: unknown;
}

export interface CheckReport {
  readonly version: typeof CORPUS_VERSION;
  readonly unicodeVersion: typeof UNICODE_VERSION;
  readonly caseCount: number;
  readonly passed: number;
  readonly failed: number;
  readonly issues: readonly CheckIssue[];
}

export type UnicodeErrorCode =
  | "invalid_text"
  | "invalid_position"
  | "invalid_direction"
  | "invalid_corpus"
  | "limit_exceeded"
  | "unsupported_runtime"
  | "backend_invariant";

export class UnicodeError extends Error {
  constructor(
    public readonly code: UnicodeErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "UnicodeError";
  }
}
