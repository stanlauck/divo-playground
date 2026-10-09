// SPDX-License-Identifier: MIT OR Apache-2.0
import test from "node:test";
import assert from "node:assert/strict";
import {
  createTheme,
  cssVariables,
  cssText,
  contrast,
  PRESETS,
} from "../dist/src/index.js";
import { sample, syntheticImage } from "../examples/fixture.mjs";
for (const preset of PRESETS)
  for (const mode of ["light", "dark"])
    for (const density of ["compact", "comfortable", "spacious"])
      test(`${preset}/${mode}/${density}: AA pairs, frozen tokens, determinism`, () => {
        const config = { version: 1, preset, mode, density },
          theme = createTheme(config);
        assert.deepEqual(theme, createTheme(config));
        assert.ok(
          Object.isFrozen(theme) &&
            Object.isFrozen(theme.colors) &&
            Object.isFrozen(theme.metrics) &&
            Object.isFrozen(theme.pairs),
        );
        for (const pair of theme.pairs) {
          assert.ok(pair.ratio >= pair.minimum, JSON.stringify(pair));
          assert.equal(
            pair.ratio,
            contrast(
              theme.colors[pair.foreground],
              theme.colors[pair.background],
            ),
          );
        }
        assert.ok(
          Object.keys(cssVariables(theme)).every((k) =>
            /^--pg11-[a-z-]+$/.test(k),
          ),
        );
      });
test("full custom HSL range gets corrected only when needed", () => {
  for (const mode of ["light", "dark"])
    for (const h of [0, 60, 120, 180, 240, 300, 360])
      for (const s of [0, 50, 100])
        for (const l of [0, 25, 50, 75, 100]) {
          const theme = createTheme({
            ...sample,
            mode,
            accent: { source: "custom", hsl: { h, s, l } },
          });
          assert.equal(theme.accent.source, "custom");
          for (const pair of theme.pairs) assert.ok(pair.ratio >= pair.minimum);
        }
});
test("cover source, missing/transparent fallback and precedence", () => {
  const config = { ...sample, accent: { source: "cover" } },
    theme = createTheme(config, syntheticImage());
  assert.equal(theme.accent.source, "cover");
  assert.equal(theme.accent.requested, "#3068b0");
  assert.equal(createTheme(config).accent.source, "fallback");
  assert.equal(
    createTheme(config, { width: 1, height: 1, pixels: new Uint8Array(4) })
      .accent.source,
    "fallback",
  );
  assert.throws(() => createTheme(sample, syntheticImage()));
});
test("mode/density change appropriate tokens without caller mutation", () => {
  const source = structuredClone(sample),
    before = structuredClone(source),
    light = createTheme(source),
    dark = createTheme({ ...source, mode: "dark" }),
    compact = createTheme({ ...source, density: "compact" });
  assert.deepEqual(source, before);
  assert.notDeepEqual(light.colors, dark.colors);
  assert.deepEqual(light.colors, compact.colors);
  assert.ok(compact.metrics.controlHeight < light.metrics.controlHeight);
});
test("CSS output cannot carry injection or fabricated snapshots", () => {
  const theme = createTheme(sample);
  assert.ok(
    cssText(theme, "custom").startsWith(":root {\n  --custom-background: #"),
  );
  for (const prefix of ["x};body{", "--x", "X", "", "x".repeat(33)])
    assert.throws(() => cssText(theme, prefix));
  assert.throws(() => cssText({ ...theme }));
  assert.ok(Object.isFrozen(cssVariables(theme)));
});
