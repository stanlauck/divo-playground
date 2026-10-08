// SPDX-License-Identifier: MIT OR Apache-2.0

import { NO_BREAK, PASS, Rules } from "@cto.af/linebreak";
import { graphemeSegments } from "unicode-segmenter/grapheme";
import {
  MAX_TEXT_UTF16,
  UNICODE_VERSION,
  UnicodeError,
  type CursorDirection,
  type Encoding,
  type Grapheme,
  type LineBreak,
  type Position,
  type TextAnalysis,
} from "./model.js";

export function scalarPositions(text: string): readonly Position[] {
  if (typeof text !== "string") {
    throw new UnicodeError("invalid_text", "Text must be a string");
  }
  if (text.length > MAX_TEXT_UTF16) {
    throw new UnicodeError("limit_exceeded", "Text exceeds the UTF-16 limit");
  }
  const positions: Position[] = [Object.freeze({ utf16: 0, utf8: 0 })];
  let utf16 = 0;
  let utf8 = 0;
  while (utf16 < text.length) {
    const point = text.codePointAt(utf16)!;
    if (point >= 0xd800 && point <= 0xdfff) {
      throw new UnicodeError(
        "invalid_text",
        "Text contains an unpaired UTF-16 surrogate",
      );
    }
    utf16 += point > 0xffff ? 2 : 1;
    utf8 += point <= 0x7f ? 1 : point <= 0x7ff ? 2 : point <= 0xffff ? 3 : 4;
    positions.push(Object.freeze({ utf16, utf8 }));
  }
  return Object.freeze(positions);
}

function lowerBound(
  positions: readonly Position[],
  offset: number,
  encoding: Encoding,
): number {
  let low = 0;
  let high = positions.length;
  while (low < high) {
    const middle = low + Math.floor((high - low) / 2);
    if (positions[middle]![encoding] < offset) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

/** Immutable index of a complete string, not a streaming or visual layout. */
export class UnicodeText {
  readonly text: string;
  readonly #positions: readonly Position[];
  readonly #graphemes: readonly Grapheme[];
  readonly #boundaries: readonly Position[];
  #lineBreaks: readonly LineBreak[] | undefined;

  constructor(text: string) {
    this.#positions = scalarPositions(text);
    this.text = text;
    const graphemes: Grapheme[] = [];
    const boundaries: Position[] = [this.#positions[0]!];
    for (const segment of graphemeSegments(text)) {
      const start = this.positionAt(segment.index, "utf16");
      const end = this.positionAt(
        segment.index + segment.segment.length,
        "utf16",
      );
      graphemes.push(Object.freeze({ text: segment.segment, start, end }));
      boundaries.push(end);
    }
    this.#graphemes = Object.freeze(graphemes);
    this.#boundaries = Object.freeze(boundaries);
    Object.freeze(this);
  }

  get length(): Position {
    return this.#positions[this.#positions.length - 1]!;
  }

  get graphemeCount(): number {
    return this.#graphemes.length;
  }

  get positions(): readonly Position[] {
    return this.#positions;
  }

  get graphemes(): readonly Grapheme[] {
    return this.#graphemes;
  }

  get boundaries(): readonly Position[] {
    return this.#boundaries;
  }

  /** Converts only scalar-aligned offsets. It never rounds or replaces text. */
  positionAt(offset: number, encoding: Encoding): Position {
    if (
      (encoding !== "utf16" && encoding !== "utf8") ||
      !Number.isSafeInteger(offset) ||
      offset < 0
    ) {
      throw new UnicodeError("invalid_position", "Invalid encoding or offset");
    }
    const index = lowerBound(this.#positions, offset, encoding);
    const position = this.#positions[index];
    if (position === undefined || position[encoding] !== offset) {
      throw new UnicodeError(
        "invalid_position",
        "Offset is out of range or inside an encoded scalar",
      );
    }
    return position;
  }

  /** Checks that both supplied offsets identify the same scalar boundary. */
  validatePosition(at: Position): Position {
    if (at === null || typeof at !== "object") {
      throw new UnicodeError(
        "invalid_position",
        "Position must be an offset pair",
      );
    }
    const position = this.positionAt(at.utf16, "utf16");
    if (position.utf8 !== at.utf8) {
      throw new UnicodeError(
        "invalid_position",
        "UTF-16 and UTF-8 offsets disagree",
      );
    }
    return position;
  }

  /**
   * Strict previous/next EGC boundary, even from inside a cluster.
   * Returns null when there is no boundary in that logical direction.
   */
  move(at: Position, direction: CursorDirection): Position | null {
    if (direction !== "previous" && direction !== "next") {
      throw new UnicodeError(
        "invalid_direction",
        "Cursor direction must be previous or next",
      );
    }
    const position = this.validatePosition(at);
    const index = lowerBound(this.#boundaries, position.utf16, "utf16");
    if (direction === "previous") {
      return this.#boundaries[index - 1] ?? null;
    }
    const exact = this.#boundaries[index]?.utf16 === position.utf16;
    return this.#boundaries[index + (exact ? 1 : 0)] ?? null;
  }

  /**
   * Untailored UAX #14 opportunities, including required end-of-text.
   * No width fitting, whitespace trimming, or discretionary-hyphen insertion.
   */
  lineBreaks(): readonly LineBreak[] {
    if (this.#lineBreaks === undefined) {
      // The backend uses native Unicode property regexes as well as its tables.
      if (process.versions.unicode !== "17.0") {
        throw new UnicodeError(
          "unsupported_runtime",
          "Line breaking requires native Unicode 17.0 properties",
        );
      }
      const breaks: LineBreak[] = [];
      const rules = new Rules();
      // 4.0.3 coalesces LB9 characters into the base and loses the ZWJ class.
      // Preserve normative LB8a using the actual last consumed code unit.
      rules.addRuleBefore("LB08a", (state) =>
        state.str.charCodeAt(state.cur.len - 1) === 0x200d ? NO_BREAK : PASS,
      );
      for (const item of rules.breaks(this.text)) {
        breaks.push(
          Object.freeze({
            at: this.positionAt(item.position, "utf16"),
            required: item.required,
          }),
        );
      }
      const final = breaks[breaks.length - 1];
      if (final?.at.utf16 !== this.length.utf16 || !final.required) {
        throw new UnicodeError(
          "backend_invariant",
          "UAX #14 backend did not emit required end-of-text",
        );
      }
      this.#lineBreaks = Object.freeze(breaks);
    }
    return this.#lineBreaks;
  }
}

export function analyzeText(text: string): TextAnalysis {
  const index = new UnicodeText(text);
  return Object.freeze({
    unicodeVersion: UNICODE_VERSION,
    graphemeCount: index.graphemeCount,
    graphemes: index.graphemes,
    cursor: Object.freeze(
      index.positions.map((at) =>
        Object.freeze({
          at,
          previous: index.move(at, "previous"),
          next: index.move(at, "next"),
        }),
      ),
    ),
    lineBreaks: index.lineBreaks(),
  });
}
