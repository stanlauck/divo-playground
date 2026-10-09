// SPDX-License-Identifier: MIT OR Apache-2.0
import test from "node:test";
import assert from "node:assert/strict";
import { runInNewContext } from "node:vm";
import { parseConfig, validateHsl, ThemeError } from "../dist/src/index.js";
import { sample } from "../examples/fixture.mjs";
test("config defaults, canonical hue, detached frozen values", () => {
  const source = structuredClone(sample);
  source.accent.hsl.h = 360;
  const result = parseConfig(source);
  source.accent.hsl.s = 0;
  assert.equal(result.accent.hsl.h, 0);
  assert.equal(result.accent.hsl.s, 70);
  assert.ok(
    Object.isFrozen(result) &&
      Object.isFrozen(result.accent) &&
      Object.isFrozen(result.accent.hsl),
  );
  const { accent, ...base } = sample;
  assert.deepEqual(parseConfig(base).accent, { source: "preset" });
});
for (const value of [
  null,
  [],
  true,
  "private input",
  3,
  new Date(),
  new Map(),
  { ...sample, secret: 1 },
  { ...sample, version: 2 },
  { ...sample, mode: "system" },
  { ...sample, preset: "custom" },
  { ...sample, density: 3 },
])
  test("invalid config safely rejects", () =>
    assert.throws(
      () => parseConfig(value),
      (error) =>
        error instanceof ThemeError &&
        !error.message.includes("private input") &&
        !error.message.includes("secret"),
    ));
for (const accent of [
  null,
  { source: "preset", hsl: { h: 1, s: 2, l: 3 } },
  { source: "cover", url: "private input" },
  { source: "custom" },
  { source: "x" },
  { source: "custom", hsl: { h: -1, s: 10, l: 20 } },
  { source: "custom", hsl: { h: 0, s: NaN, l: 30 } },
  { source: "custom", hsl: { h: 0, s: 30, l: Infinity } },
])
  test("invalid accent safely rejects", () =>
    assert.throws(() => parseConfig({ ...sample, accent }), ThemeError));
test("accessors/toJSON are not invoked", () => {
  let calls = 0;
  const value = { ...sample };
  Object.defineProperty(value, "mode", {
    get() {
      calls++;
      return "dark";
    },
    enumerable: true,
  });
  assert.throws(() => parseConfig(value), ThemeError);
  assert.throws(
    () =>
      parseConfig({
        ...sample,
        toJSON() {
          calls++;
        },
      }),
    ThemeError,
  );
  assert.equal(calls, 0);
});
test("null-prototype records are accepted, inherited data is not", () => {
  assert.equal(
    parseConfig(Object.assign(Object.create(null), sample)).preset,
    "paper",
  );
  assert.throws(() => parseConfig(Object.create(sample)), ThemeError);
});
test("HSL finite ranges and scalar fields", () => {
  assert.equal(validateHsl({ h: 360, s: 0, l: 0 }).h, 0);
  for (const value of [
    { h: 361, s: 1, l: 1 },
    { h: 0, s: 101, l: 1 },
    { h: 0, s: 1, l: -1 },
    { h: 0, s: 1, l: "1" },
    { h: 0, s: 1, l: 1, a: 1 },
  ])
    assert.throws(() => validateHsl(value), ThemeError);
});
test("ordinary nested configs work across realms without accepting exotic prototypes", () => {
  const foreign = runInNewContext(`(${JSON.stringify(sample)})`);
  assert.notEqual(Object.getPrototypeOf(foreign), Object.prototype);
  assert.deepEqual(parseConfig(foreign), parseConfig(sample));
  const inherited = runInNewContext(`Object.create(${JSON.stringify(sample)})`);
  assert.throws(() => parseConfig(inherited), ThemeError);
  const fake = Object.assign(
    Object.create(Object.assign(Object.create(null), { constructor: Object })),
    sample,
  );
  assert.throws(() => parseConfig(fake), ThemeError);
  const foreignSubclass = runInNewContext(
    `Object.assign(new (class Extra extends Object {})(), ${JSON.stringify(sample)})`,
  );
  assert.throws(() => parseConfig(foreignSubclass), ThemeError);
});
