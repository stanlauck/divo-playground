// SPDX-License-Identifier: MIT OR Apache-2.0
import test from "node:test";
import assert from "node:assert/strict";
import {
  hslToHex,
  hexToHsl,
  luminance,
  contrast,
  toHex,
  fromHex,
} from "../dist/src/index.js";
const close = (a, b) => assert.ok(Math.abs(a - b) < 1e-10, `${a} != ${b}`);
for (const [h, expected] of [
  [0, "#ff0000"],
  [60, "#ffff00"],
  [120, "#00ff00"],
  [180, "#00ffff"],
  [240, "#0000ff"],
  [300, "#ff00ff"],
  [360, "#ff0000"],
])
  test("standard HSL primary/secondary", () =>
    assert.equal(hslToHex({ h, s: 100, l: 50 }), expected));
test("known WCAG ratios/luminance", () => {
  close(contrast("#000000", "#ffffff"), 21);
  close(contrast("#777777", "#777777"), 1);
  close(luminance("#ff0000"), 0.2126);
  close(luminance("#000000"), 0);
  close(luminance("#ffffff"), 1);
  close(contrast("#767676", "#ffffff"), 4.542224959605253);
});
test("RGB/HSL quantized roundtrips independent of locale", () => {
  for (let r = 0; r <= 255; r += 51)
    for (let g = 0; g <= 255; g += 51)
      for (let b = 0; b <= 255; b += 51) {
        const hex = toHex({ r, g, b });
        assert.equal(hslToHex(hexToHsl(hex)), hex);
        assert.deepEqual(fromHex(hex), { r, g, b });
      }
});
for (const value of ["red", "#fff", "#00000000", "url(private)", null, 3])
  test("invalid hex rejects", () => assert.throws(() => fromHex(value)));
test("invalid RGB rejects", () => {
  for (const r of [-1, 256, NaN, 0.1])
    assert.throws(() => toHex({ r, g: 0, b: 0 }));
});
