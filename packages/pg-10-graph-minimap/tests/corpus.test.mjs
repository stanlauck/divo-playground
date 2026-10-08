// SPDX-License-Identifier: MIT OR Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { makeDocument } from "../examples/fixture.mjs";
import { makeSchema } from "../examples/make-data.mjs";
import { prepareDocument } from "../dist/src/model.js";
test("synthetic document and schema reproduce independently", async () => {
  const sample = JSON.parse(
    await readFile(
      new URL("../samples/synthetic.json", import.meta.url),
      "utf8",
    ),
  );
  const schema = JSON.parse(
    await readFile(new URL("../schema.json", import.meta.url), "utf8"),
  );
  assert.deepEqual(sample, makeDocument());
  assert.deepEqual(schema, makeSchema());
  assert.equal(prepareDocument(sample).graph.nodes.length, 16);
});
