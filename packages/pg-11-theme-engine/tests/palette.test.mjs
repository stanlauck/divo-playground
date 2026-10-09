// SPDX-License-Identifier: MIT OR Apache-2.0
import test from "node:test";
import assert from "node:assert/strict";
import { runInNewContext } from "node:vm";
import { dominantAccent, IMAGE_LIMITS } from "../dist/src/index.js";
import { syntheticImage } from "../examples/fixture.mjs";
test("synthetic cover picks weighted majority, no mutation", () => {
  const image = syntheticImage(),
    before = image.pixels.slice(),
    result = dominantAccent(image);
  assert.equal(result.color, "#3068b0");
  assert.equal(result.sampled, 256);
  assert.equal(result.visible, 256);
  assert.deepEqual(image.pixels, before);
  assert.ok(Object.isFrozen(result));
});
test("transparency, black/white/grayscale are handled explicitly", () => {
  const image = {
    width: 3,
    height: 1,
    pixels: new Uint8Array([255, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 128]),
  };
  assert.equal(dominantAccent(image).color, "#000000");
  image.pixels.fill(0);
  assert.equal(dominantAccent(image).color, null);
  image.pixels.set([255, 255, 255, 255]);
  assert.equal(dominantAccent(image).color, "#ffffff");
});
test("equal-weight ties choose lowest RGB bin deterministically", () => {
  const pixels = new Uint8Array([255, 0, 0, 255, 0, 0, 255, 255]);
  assert.equal(
    dominantAccent({ width: 2, height: 1, pixels }).color,
    "#0000ff",
  );
  assert.equal(
    dominantAccent({
      width: 2,
      height: 1,
      pixels: new Uint8Array([...pixels.subarray(4), ...pixels.subarray(0, 4)]),
    }).color,
    "#0000ff",
  );
});
test("bin color averages visible samples by alpha", () => {
  assert.equal(
    dominantAccent({
      width: 2,
      height: 1,
      pixels: new Uint8Array([0, 0, 0, 255, 7, 7, 7, 255]),
    }).color,
    "#040404",
  );
});
for (const value of [
  null,
  { width: 0, height: 1, pixels: new Uint8Array() },
  { width: 5000, height: 1, pixels: new Uint8Array() },
  { width: 4096, height: 4096, pixels: new Uint8Array() },
  { width: 1, height: 1, pixels: [0, 0, 0, 0] },
  { width: 1, height: 1, pixels: new Uint16Array(4) },
  { width: 1, height: 1, pixels: new Uint8Array(3) },
])
  test("invalid RGBA input rejects", () =>
    assert.throws(() => dominantAccent(value)));
test("shared buffers and subclass arrays are not accepted", () => {
  class Extra extends Uint8Array {}
  assert.throws(() =>
    dominantAccent({ width: 1, height: 1, pixels: new Extra(4) }),
  );
  assert.throws(() =>
    dominantAccent({
      width: 1,
      height: 1,
      pixels: new Uint8Array(new SharedArrayBuffer(4)),
    }),
  );
});
test("large image uses bounded deterministic stratified sampling", () => {
  const image = syntheticImage(512, 256);
  assert.equal(dominantAccent(image).sampled, IMAGE_LIMITS.samples);
  assert.deepEqual(dominantAccent(image), dominantAccent(image));
});
test("ordinary byte arrays and image records work across realms", () => {
  for (const kind of ["Uint8Array", "Uint8ClampedArray"]) {
    const foreign = runInNewContext(
      `({width:1,height:1,pixels:new ${kind}([48,104,176,255])})`,
    );
    assert.equal(dominantAccent(foreign).color, "#3068b0");
  }
  for (const expression of [
    "new (class Extra extends Uint8Array {})(4)",
    "new Uint8Array(new SharedArrayBuffer(4))",
    "new Uint16Array(4)",
    "new DataView(new ArrayBuffer(4))",
  ])
    assert.throws(() =>
      dominantAccent(
        runInNewContext(`({width:1,height:1,pixels:${expression}})`),
      ),
    );
});
test("byte-view metadata accessors, tag spoofs and detached buffers reject safely", () => {
  let calls = 0;
  for (const key of ["buffer", "length", "byteLength", "byteOffset"]) {
    const pixels = new Uint8Array(4);
    Object.defineProperty(pixels, key, {
      get() {
        calls++;
      },
    });
    assert.throws(() => dominantAccent({ width: 1, height: 1, pixels }));
  }
  assert.equal(calls, 0);
  const fake = {
    [Symbol.toStringTag]: "Uint8Array",
    length: 4,
    buffer: new ArrayBuffer(4),
  };
  assert.throws(() => dominantAccent({ width: 1, height: 1, pixels: fake }));
  const wrongKind = new Float32Array(4);
  Object.setPrototypeOf(wrongKind, Uint8Array.prototype);
  assert.throws(() =>
    dominantAccent({ width: 1, height: 1, pixels: wrongKind }),
  );
  const detached = new Uint8Array(4);
  structuredClone(detached.buffer, { transfer: [detached.buffer] });
  assert.throws(() =>
    dominantAccent({ width: 1, height: 1, pixels: detached }),
  );
});
