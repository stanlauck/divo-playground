// SPDX-License-Identifier: MIT OR Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";
import {
  projection,
  worldToMap,
  mapToWorld,
  panViewport,
  centerViewport,
  zoomViewport,
  fitViewport,
  contains,
} from "../dist/src/geometry.js";
import { prepareGraph } from "../dist/src/model.js";
const rect = { x: -200, y: 50, width: 400, height: 100 };
const close = (a, b) => assert.ok(Math.abs(a - b) < 1e-8, `${a} ≠ ${b}`);
test("aspect-preserving centered projection and inverse", () => {
  const t = projection(rect, 240, 160);
  close(t.scale, 224 / 400);
  const p = { x: -123.5, y: 89.4 };
  const roundtrip = mapToWorld(worldToMap(p, t), t);
  close(roundtrip.x, p.x);
  close(roundtrip.y, p.y);
  close(worldToMap({ x: 0, y: 100 }, t).x, 120);
  close(worldToMap({ x: 0, y: 100 }, t).y, 80);
});
test("tiny nodes near coordinate limits keep valid bounds and stable mapping", () => {
  const node = {
    id: "tiny",
    x: 999999999,
    y: -999999999,
    width: 1e-6,
    height: 1e-6,
  };
  const graph = prepareGraph({ version: 1, nodes: [node], edges: [] });
  assert.ok(graph.bounds.width >= 1e-6 && graph.bounds.height >= 1e-6);
  const t = projection(graph.bounds, 320, 200);
  // Subtract the world origin before scaling instead of cancelling huge screen values.
  const topLeft = worldToMap(node, t);
  close(topLeft.x, 68);
  close(topLeft.y, 8);
  assert.deepEqual(mapToWorld(topLeft, t), { x: node.x, y: node.y });
  const next = worldToMap(
    { x: node.x + node.width, y: node.y + node.height },
    t,
  );
  assert.ok(
    next.x > topLeft.x && next.y > topLeft.y && next.x <= 252 && next.y <= 192,
  );
});
for (const [width, height, padding] of [
  [0, 100, 8],
  [100, 0, 8],
  [4096, 100, 8],
  [100, 100, 50],
  [NaN, 100, 0],
  [100, 100, -1],
]) {
  test("invalid surface dimensions", () =>
    assert.throws(() => projection(rect, width, height, padding), RangeError));
}
test("panning and centering retain viewport size", () => {
  assert.deepEqual(panViewport(rect, 10, -30), { ...rect, x: -190, y: 20 });
  assert.deepEqual(centerViewport(rect, { x: 0, y: 0 }), {
    ...rect,
    x: -200,
    y: -50,
  });
});
test("zoom preserves anchor fractional position and aspect ratio", () => {
  const anchor = { x: -100, y: 70 },
    next = zoomViewport(rect, 0.5, anchor);
  close((anchor.x - next.x) / next.width, (anchor.x - rect.x) / rect.width);
  close((anchor.y - next.y) / next.height, (anchor.y - rect.y) / rect.height);
  close(next.width / next.height, rect.width / rect.height);
});
for (const factor of [0, -1, NaN, Infinity]) {
  test("invalid zoom factor", () =>
    assert.throws(
      () => zoomViewport(rect, factor, { x: 0, y: 0 }),
      RangeError,
    ));
}
test("fit contains bounds and retains viewport aspect ratio", () => {
  const bounds = { x: -1000, y: -2000, width: 400, height: 3000 };
  const fitted = fitViewport(rect, bounds);
  assert.ok(contains(fitted, bounds));
  assert.ok(
    contains(fitted, {
      x: bounds.x + bounds.width,
      y: bounds.y + bounds.height,
    }),
  );
  close(fitted.width / fitted.height, rect.width / rect.height);
});
test("fit contains graph when the old aspect would exceed the numeric domain", () => {
  const viewport = { x: -1e9, y: 0, width: 2e9, height: 1e-6 };
  const bounds = { x: 0, y: 0, width: 10, height: 10 };
  const fitted = fitViewport(viewport, bounds);
  assert.ok(contains(fitted, bounds) && contains(fitted, { x: 10, y: 10 }));
  assert.equal(fitted.width, 2e9);
  assert.equal(fitted.height, 10);
});
test("numeric-domain clamps do not clamp panning to graph bounds", () => {
  assert.equal(panViewport(rect, 100_000, 0).x, 99_800);
  const next = panViewport(rect, 1e20, -1e20);
  assert.equal(next.x + next.width, 1e9);
  assert.equal(next.y, -1e9);
});
test("zero zoom extremes remain finite positive rectangles", () => {
  for (const factor of [1e-300, 1e300]) {
    const next = zoomViewport(rect, factor, { x: 0, y: 0 });
    assert.ok(Number.isFinite(next.width) && next.width > 0);
  }
});
