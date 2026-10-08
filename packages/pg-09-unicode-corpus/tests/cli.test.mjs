// SPDX-License-Identifier: MIT OR Apache-2.0

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const checker = fileURLToPath(
  new URL("../dist/examples/check.js", import.meta.url),
);
const generator = fileURLToPath(
  new URL("../examples/make-corpus.mjs", import.meta.url),
);
const original = await readFile(
  new URL("../corpus/corpus.json", import.meta.url),
);
const run = (script, ...args) =>
  spawnSync(process.execPath, [script, ...args], {
    encoding: "utf8",
    timeout: 20_000,
  });

async function temporary(action) {
  const directory = await mkdtemp(join(tmpdir(), "pg09-test-"));
  try {
    return await action(directory);
  } finally {
    // Only the fresh test-owned directory is removed.
    await rm(directory, { recursive: true, force: true });
  }
}

test("CLI accepts a UTF-8 corpus, reports every case and is cwd-independent", async () => {
  await temporary(async (directory) => {
    const path = join(directory, "data.json");
    await writeFile(path, original);
    const result = run(checker, path);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stderr, "");
    const report = JSON.parse(result.stdout);
    assert.equal(report.caseCount, 44);
    assert.equal(report.passed, 44);
    assert.equal(report.failed, 0);
  });
});

test("CLI errors on missing/extra arguments and does not leak input paths", () => {
  for (const args of [[], ["not-present"], ["first", "second"]]) {
    const result = run(checker, ...args);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes("not-present"));
  }
});

test("CLI fatal UTF-8 decoding rejects corruption instead of creating replacement text", async () => {
  await temporary(async (directory) => {
    const path = join(directory, "bad.json");
    const bytes = Buffer.from(original);
    bytes[10] = 0xff;
    await writeFile(path, bytes);
    const result = run(checker, path);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.equal(result.stderr, "Unable to read UTF-8 corpus\n");
  });
});

test("CLI accepts an input BOM without changing embedded text offsets", async () => {
  await temporary(async (directory) => {
    const path = join(directory, "bom.json");
    await writeFile(
      path,
      Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), original]),
    );
    const result = run(checker, path);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(JSON.parse(result.stdout).passed, 44);
  });
});

test("CLI rejects a structurally valid but wrong oracle with a nonzero report", async () => {
  await temporary(async (directory) => {
    const path = join(directory, "mismatch.json");
    const data = JSON.parse(original.toString("utf8"));
    const sample = data.cases.find((item) => item.id === "ascii-space");
    sample.expected.lineBreaks.shift();
    await writeFile(path, JSON.stringify(data));
    const result = run(checker, path);
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.stderr, "");
    const report = JSON.parse(result.stdout);
    assert.equal(report.failed, 1);
    assert.equal(report.issues[0].caseId, "ascii-space");
    assert.equal(report.issues[0].field, "lineBreaks");
  });
});

test("generator reproduces exact bytes and refuses overwrite or extra arguments", async () => {
  await temporary(async (directory) => {
    const path = join(directory, "new.json");
    let result = run(generator, path);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(await readFile(path), original);
    result = run(generator, path);
    assert.equal(result.status, 1);
    assert.deepEqual(await readFile(path), original);
    const other = join(directory, "other.json");
    result = run(generator, other, "extra");
    assert.equal(result.status, 1);
    await assert.rejects(readFile(other), { code: "ENOENT" });
  });
});
