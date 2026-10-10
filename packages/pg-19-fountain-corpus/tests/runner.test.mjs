// SPDX-License-Identifier: MIT OR Apache-2.0

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";
import test from "node:test";
import { compareElements, loadCorpus, normalizeFountain, validateExpected } from "../dist/index.js";

const execFileAsync = promisify(execFile);
const root = new URL("..", import.meta.url);
const corpus = loadCorpus();
const cli = new URL("dist/cli.js", root).pathname;
const run = async (...args) => execFileAsync(process.execPath, [cli, "run", ...args], { cwd: new URL(root).pathname });

test("corpus has the required focused cases and kitchen sink", () => {
  assert.ok(corpus.some((item) => item.id === "18-kitchen-sink"));
  assert.ok(corpus.some((item) => item.id === "02-scene-headings"));
  assert.ok(corpus.length >= 20);
});

for (const item of corpus) {
  test(`expected JSON is structurally valid: ${item.id}`, () => {
    assert.deepEqual(validateExpected(item.expected), []);
    assert.equal(item.expected.version, 1);
  });
}

test("every fountain file has a paired expected file", async () => {
  for (const item of corpus) assert.ok((await readFile(item.expectedPath, "utf8")).includes('"elements"'));
});

test("BOM/CRLF case normalizes to the action LF expectation", () => {
  const crlf = corpus.find((item) => item.id === "15-line-endings");
  const lf = corpus.find((item) => item.id === "03-action");
  assert.ok(crlf && lf);
  assert.deepEqual(crlf.expected, lf.expected);
  assert.equal(crlf.text, lf.text);
});

test("normalization removes BOM and converts CRLF", () => assert.equal(normalizeFountain("\uFEFFa\r\nb\r"), "a\nb\n"));

test("strict comparison reports a passing identical list", () => {
  const expected = corpus[0].expected.elements;
  const result = compareElements(expected, structuredClone(expected));
  assert.equal(result.equal, true);
  assert.equal(result.diff, "");
});

test("comparison reports first differing index and readable lists", () => {
  const expected = [{ type: "action", text: "one" }];
  const result = compareElements(expected, [{ type: "action", text: "two" }]);
  assert.equal(result.firstDifference?.index, 0);
  assert.match(result.diff, /first difference at index 0/);
  assert.match(result.diff, /expected/);
  assert.match(result.diff, /actual/);
});

test("ignore removes requested types from both lists", () => {
  const result = compareElements([{ type: "note", text: "x" }, { type: "action", text: "a" }], [{ type: "action", text: "a" }], { ignore: ["note"] });
  assert.equal(result.equal, true);
});

test("loose whitespace collapses text runs", () => {
  const result = compareElements([{ type: "action", text: "a b" }], [{ type: "action", text: "a\n  b" }], { looseWhitespace: true });
  assert.equal(result.equal, true);
});

test("styled presentation field is ignored", () => {
  const result = compareElements([{ type: "action", text: "x" }], [{ type: "action", text: "x", styled: [{ bold: true }] }]);
  assert.equal(result.equal, true);
});

test("echo parser passes every corpus case", async () => {
  const result = await run("--parser", "node examples/echo-expected.mjs");
  assert.equal(result.stdout.includes("Summary: 20 passed, 0 failed, 20 total"), true);
});

test("echo parser supports one selected case", async () => {
  const result = await run("--parser", "node examples/echo-expected.mjs", "--only", "08-centered", "--json");
  const report = JSON.parse(result.stdout);
  assert.deepEqual(report.summary, { total: 1, passed: 1, failed: 0 });
});

test("file substitution mode passes every case", async () => {
  const result = await run("--parser", "node examples/file-echo.mjs {file}", "--json");
  assert.deepEqual(JSON.parse(result.stdout).summary, { total: 20, passed: 20, failed: 0 });
});

test("list prints stable case IDs", async () => {
  const result = await execFileAsync(process.execPath, [cli, "list"], { cwd: new URL(root).pathname });
  assert.equal(result.stdout.split("\n").filter(Boolean).length, 20);
  assert.equal(result.stdout.split("\n")[0], "01-title-page");
});

test("wrong parser exits one and includes a diff", async () => {
  await assert.rejects(() => run("--parser", "node examples/wrong-parser.mjs"), (error) => {
    assert.equal(error.code, 1);
    assert.match(error.stdout, /FAIL 01-title-page/);
    assert.match(error.stdout, /first difference at index 0/);
    return true;
  });
});

test("ignore option lets a note-stripping adapter pass the note case", async () => {
  const result = await run("--parser", "node examples/echo-expected.mjs", "--only", "12-notes", "--ignore", "note");
  assert.match(result.stdout, /Summary: 1 passed/);
});

test("JSON summary has per-case booleans", async () => {
  const result = await run("--parser", "node examples/echo-expected.mjs", "--only", "17-empty", "--json");
  const report = JSON.parse(result.stdout);
  assert.equal(report.results[0].pass, true);
  assert.deepEqual(report.summary, { total: 1, passed: 1, failed: 0 });
});

test("parser JSON without elements fails cleanly", async () => {
  await assert.rejects(() => run("--parser", "node -e 'process.stdin.resume(); process.stdin.on(\"end\",()=>console.log(\"{}\"))'", "--only", "17-empty"), (error) => {
    assert.equal(error.code, 1);
    assert.match(error.stdout, /parser JSON must contain an elements array/);
    return true;
  });
});
