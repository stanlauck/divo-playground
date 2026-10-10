// SPDX-License-Identifier: MIT OR Apache-2.0

import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const ELEMENT_TYPES = [
  "scene_heading", "action", "character", "parenthetical", "dialogue",
  "dual_dialogue_begin", "dual_dialogue_end", "transition", "centered",
  "lyric", "page_break", "section", "synopsis", "note", "boneyard",
] as const;
export type ElementType = (typeof ELEMENT_TYPES)[number];

export interface Element {
  type: ElementType;
  text: string;
  scene_number?: string;
  depth?: number;
  dual?: boolean;
  forced?: boolean;
  [key: string]: unknown;
}

export interface TitlePageEntry { key: string; value: string }
export interface ExpectedDocument { version: 1; title_page: TitlePageEntry[]; elements: Element[] }
export interface CorpusCase {
  id: string;
  fountainPath: string;
  expectedPath: string;
  text: string;
  expected: ExpectedDocument;
}

export interface CompareOptions { ignore?: Iterable<ElementType | string>; looseWhitespace?: boolean }
export interface Comparison {
  equal: boolean;
  expected: Element[];
  actual: Element[];
  firstDifference?: { index: number; expected?: Element | undefined; actual?: Element | undefined };
  diff: string;
}

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const corpusRoot = join(packageRoot, "corpus");

export function normalizeFountain(input: string): string {
  return input.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
}

export function loadCorpus(): CorpusCase[] {
  const names = readdirSync(corpusRoot)
    .filter((name) => name.endsWith(".fountain"))
    .sort();
  return names.map((name) => {
    const base = name.slice(0, -".fountain".length);
    const fountainPath = join(corpusRoot, name);
    const expectedPath = join(corpusRoot, `${base}.expected.json`);
    const expected = JSON.parse(readFileSync(expectedPath, "utf8")) as ExpectedDocument;
    return { id: base, fountainPath, expectedPath, text: normalizeFountain(readFileSync(fountainPath, "utf8")), expected };
  });
}

function normalizedElement(element: Element, looseWhitespace: boolean): Element {
  const copy = { ...element };
  delete copy.styled;
  if (looseWhitespace) copy.text = copy.text.replace(/\s+/g, " ");
  return copy;
}

function filtered(elements: Element[], options: CompareOptions): Element[] {
  const ignored = new Set(options.ignore ?? []);
  return elements.filter((element) => !ignored.has(element.type)).map((element) => normalizedElement(element, options.looseWhitespace === true));
}

export function compareElements(expected: Element[], actual: Element[], options: CompareOptions = {}): Comparison {
  const left = filtered(expected, options);
  const right = filtered(actual, options);
  let index = 0;
  while (index < left.length && index < right.length && JSON.stringify(left[index]) === JSON.stringify(right[index])) index += 1;
  const equal = index === left.length && index === right.length;
  const firstDifference = equal ? undefined : { index, expected: left[index], actual: right[index] };
  const diff = equal ? "" : readableDiff(left, right, index);
  return firstDifference === undefined ? { equal, expected: left, actual: right, diff } : { equal, expected: left, actual: right, firstDifference, diff };
}

function readableDiff(expected: Element[], actual: Element[], index: number): string {
  const lines = [`first difference at index ${index}`, `expected: ${JSON.stringify(expected[index] ?? "<end>")}`, `actual:   ${JSON.stringify(actual[index] ?? "<end>")}`, "element lists:"];
  lines.push(`  expected ${JSON.stringify(expected)}`);
  lines.push(`  actual   ${JSON.stringify(actual)}`);
  return lines.join("\n");
}

export function validateExpected(value: unknown): string[] {
  const issues: string[] = [];
  if (typeof value !== "object" || value === null || Array.isArray(value)) return ["document must be an object"];
  const doc = value as Record<string, unknown>;
  if (doc.version !== 1) issues.push("version must be 1");
  if (!Array.isArray(doc.title_page)) issues.push("title_page must be an array");
  else for (const [i, entry] of doc.title_page.entries()) {
    if (typeof entry !== "object" || entry === null || typeof (entry as Record<string, unknown>).key !== "string" || typeof (entry as Record<string, unknown>).value !== "string") issues.push(`title_page[${i}] must have string key and value`);
  }
  if (!Array.isArray(doc.elements)) issues.push("elements must be an array");
  else for (const [i, item] of doc.elements.entries()) {
    if (typeof item !== "object" || item === null) { issues.push(`elements[${i}] must be an object`); continue; }
    const element = item as Record<string, unknown>;
    if (typeof element.type !== "string" || !(ELEMENT_TYPES as readonly string[]).includes(element.type)) issues.push(`elements[${i}].type is invalid`);
    if (typeof element.text !== "string") issues.push(`elements[${i}].text must be a string`);
    if (element.scene_number !== undefined && typeof element.scene_number !== "string") issues.push(`elements[${i}].scene_number must be a string`);
    if (element.depth !== undefined && (!Number.isInteger(element.depth) || (element.depth as number) < 1)) issues.push(`elements[${i}].depth must be a positive integer`);
    for (const key of ["dual", "forced"]) if (element[key] !== undefined && typeof element[key] !== "boolean") issues.push(`elements[${i}].${key} must be boolean`);
  }
  return issues;
}

export function formatComparison(result: Comparison): string { return result.equal ? "PASS" : result.diff; }
