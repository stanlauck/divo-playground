// SPDX-License-Identifier: MIT OR Apache-2.0
import test from "node:test";
import assert from "node:assert/strict";
import { decodeCover } from "../dist/src/browser.js";
test("browser module imports under Node without browser side effects", () =>
  assert.equal(typeof decodeCover, "function"));
test("unsupported/empty/large Blobs reject before native decode", async () => {
  for (const value of [
    null,
    new Blob(),
    new Blob(["private input"]),
    new Blob([new Uint8Array(8_388_609)]),
  ])
    await assert.rejects(
      () => decodeCover(value),
      (error) => !error.message.includes("private input"),
    );
});
test("oversized PNG header rejects before browser allocation", async () => {
  const header = new Uint8Array(33);
  header.set([137, 80, 78, 71, 13, 10, 26, 10]);
  new DataView(header.buffer).setUint32(8, 13);
  new DataView(header.buffer).setUint32(12, 0x49484452);
  new DataView(header.buffer).setUint32(16, 0xffffffff);
  new DataView(header.buffer).setUint32(20, 1);
  await assert.rejects(
    () => decodeCover(new Blob([header])),
    (error) => error.code === "dimensions",
  );
});
test("unknown JPEG markers do not produce unsafe errors", async () => {
  await assert.rejects(
    () => decodeCover(new Blob([new Uint8Array([255, 216, 0, 0, 0, 0])])),
    (error) => ["image_header", "dimensions"].includes(error.code),
  );
});
