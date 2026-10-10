// SPDX-License-Identifier: MIT OR Apache-2.0

import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { compareElements, formatComparison, loadCorpus, normalizeFountain, type CompareOptions, type Element, type ElementType } from "./index.js";

interface RunOptions extends CompareOptions { parser: string; only?: string; json: boolean }
interface CaseResult { id: string; pass: boolean; comparison?: ReturnType<typeof compareElements>; error?: string }

function valueAfter(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function shellQuote(value: string): string { return `'${value.replaceAll("'", "'\\''")}'`; }

function parseRunOptions(args: string[]): RunOptions {
  const parser = valueAfter(args, "--parser");
  if (parser === undefined) throw new Error("run requires --parser <command>");
  const ignore = (valueAfter(args, "--ignore") ?? "").split(",").map((value) => value.trim()).filter(Boolean) as ElementType[];
  const options: RunOptions = { parser, ignore, looseWhitespace: args.includes("--loose-whitespace"), json: args.includes("--json") };
  const only = valueAfter(args, "--only");
  if (only !== undefined) options.only = only;
  return options;
}

function invoke(command: string, input: string): Promise<{ stdout: string; stderr: string; code: number }> {
  return new Promise((resolve) => {
    const child = spawn(command, { shell: true });
    let stdout = ""; let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => { stdout += chunk.toString(); });
    child.stderr.on("data", (chunk: Buffer) => { stderr += chunk.toString(); });
    child.on("error", (error: Error) => resolve({ stdout, stderr: `${stderr}${error.message}`, code: 1 }));
    child.on("close", (code) => resolve({ stdout, stderr, code: code ?? 1 }));
    child.stdin.end(input);
  });
}

export async function runParser(parser: string, input: string, filePath?: string): Promise<unknown> {
  const command = filePath === undefined ? parser : parser.replaceAll("{file}", shellQuote(filePath));
  const result = await invoke(command, input);
  if (result.code !== 0) throw new Error(`parser exited ${result.code}${result.stderr.trim() ? `: ${result.stderr.trim()}` : ""}`);
  try { return JSON.parse(result.stdout) as unknown; }
  catch { throw new Error(`parser stdout is not valid JSON: ${result.stdout.slice(0, 240)}`); }
}

function elementsFrom(value: unknown): Element[] {
  if (typeof value !== "object" || value === null || !Array.isArray((value as { elements?: unknown }).elements)) throw new Error("parser JSON must contain an elements array");
  return (value as { elements: Element[] }).elements;
}

export async function runCorpus(options: RunOptions): Promise<CaseResult[]> {
  const cases = loadCorpus().filter((item) => options.only === undefined || item.id === options.only);
  const results: CaseResult[] = [];
  for (const item of cases) {
    try {
      const raw = await readFile(item.fountainPath, "utf8");
      const value = await runParser(options.parser, normalizeFountain(raw), item.fountainPath);
      const comparison = compareElements(item.expected.elements, elementsFrom(value), options);
      results.push({ id: item.id, pass: comparison.equal, comparison });
    } catch (error) {
      results.push({ id: item.id, pass: false, error: error instanceof Error ? error.message : String(error) });
    }
  }
  return results;
}

async function main(): Promise<void> {
  const [subcommand, ...args] = process.argv.slice(2);
  if (subcommand === "list" || args.includes("--list")) { for (const item of loadCorpus()) console.log(item.id); return; }
  if (subcommand !== "run") throw new Error("usage: cli.js run --parser <command> [options]");
  const options = parseRunOptions(args);
  const results = await runCorpus(options);
  const passed = results.filter((item) => item.pass).length;
  const summary = { total: results.length, passed, failed: results.length - passed };
  if (options.json) {
    console.log(JSON.stringify({ results: results.map((item) => ({ id: item.id, pass: item.pass, ...(item.error === undefined ? {} : { error: item.error }) })), summary }));
  } else {
    for (const result of results) console.log(`${result.pass ? "PASS" : "FAIL"} ${result.id}${result.error === undefined && result.pass ? "" : `\n${result.error ?? formatComparison(result.comparison!)}`}`);
    console.log(`Summary: ${summary.passed} passed, ${summary.failed} failed, ${summary.total} total`);
  }
  if (summary.failed > 0) process.exitCode = 1;
}

if (process.argv[1]?.endsWith("cli.js")) main().catch((error: unknown) => { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 2; });
