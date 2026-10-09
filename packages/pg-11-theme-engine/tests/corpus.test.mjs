// SPDX-License-Identifier: MIT OR Apache-2.0
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { schema } from "../examples/make-data.mjs";
import { sample } from "../examples/fixture.mjs";
import { parseConfig } from "../dist/src/index.js";
test("schema and sample reproduce offline", async () => {
  assert.deepEqual(
    JSON.parse(
      await readFile(new URL("../schema.json", import.meta.url), "utf8"),
    ),
    schema,
  );
  const value = JSON.parse(
    await readFile(new URL("../samples/custom.json", import.meta.url), "utf8"),
  );
  assert.deepEqual(value, sample);
  assert.deepEqual(parseConfig(value), sample);
});
