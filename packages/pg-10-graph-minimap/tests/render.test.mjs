// SPDX-License-Identifier: MIT OR Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { GraphMinimap, prepareGraph } from "../dist/src/index.js";
import {
  DEFAULT_PALETTE,
  paintGraph,
  paintViewport,
  setupCanvas,
} from "../dist/src/painter.js";
import { projection } from "../dist/src/geometry.js";
import { makeGraph } from "../examples/fixture.mjs";
const graph = prepareGraph(makeGraph(10));
const viewport = { x: 0, y: 0, width: 100, height: 50 };
test("DPR canvas allocation never exceeds its pixel budget after rounding", () => {
  const context = { setTransform() {} };
  for (const [width, height] of [
    [500, 2048],
    [2048, 2048],
    [1700.2, 1820.1],
    [32, 32],
  ]) {
    const canvas = { width: 0, height: 0, getContext: () => context };
    assert.equal(setupCanvas(canvas, width, height, 3), context);
    assert.ok(canvas.width * canvas.height <= 4_194_304);
    assert.ok(canvas.width >= width - 1 && canvas.height >= height - 1);
  }
});
test("SSR has no canvas/browser side effects, labels and buttons", () => {
  const html = renderToString(
    createElement(GraphMinimap, { graph, viewport, onViewportChange() {} }),
  );
  assert.equal((html.match(/<canvas/g) ?? []).length, 2);
  assert.ok(
    html.includes("Zoom in") &&
      html.includes("Fit graph") &&
      html.includes("pan and zoom"),
  );
  assert.ok(html.replace(/<!--.*?-->/g, "").includes("10 nodes"));
});
test("fake prepared input is rejected", () =>
  assert.throws(
    () =>
      renderToString(
        createElement(GraphMinimap, {
          graph: { ...graph },
          viewport,
          onViewportChange() {},
        }),
      ),
    /prepared_graph/,
  ));
test("invalid palette, viewport and size fail before mounting", () => {
  for (const props of [
    { viewport: { ...viewport, width: -1 } },
    { width: Infinity },
    { palette: { ...DEFAULT_PALETTE, edges: "url(secret)" } },
    { palette: { nodes: "#000000" } },
    { palette: { ...DEFAULT_PALETTE, extra: "#000000" } },
  ])
    assert.throws(() =>
      renderToString(
        createElement(GraphMinimap, {
          graph,
          viewport,
          onViewportChange() {},
          ...props,
        }),
      ),
    );
});
test("palette accessors are not executed", () => {
  let calls = 0;
  const palette = { ...DEFAULT_PALETTE };
  Object.defineProperty(palette, "edges", {
    get() {
      calls++;
      return "#000000";
    },
  });
  assert.throws(() =>
    renderToString(
      createElement(GraphMinimap, {
        graph,
        viewport,
        palette,
        onViewportChange() {},
      }),
    ),
  );
  assert.equal(calls, 0);
});
test("disabled surface permits native touch scrolling", () => {
  for (const disabled of [true, false]) {
    const html = renderToString(
      createElement(GraphMinimap, {
        graph,
        viewport,
        disabled,
        onViewportChange() {},
      }),
    );
    assert.ok(html.includes(`touch-action:${disabled ? "auto" : "none"}`));
  }
});
test("static graph painter is separate from constant-work overlay", () => {
  const calls = [];
  const context = new Proxy(
    {},
    {
      get(_target, key) {
        return (...args) => calls.push([key, args]);
      },
      set() {
        return true;
      },
    },
  );
  const t = projection(graph.bounds, 240, 160);
  paintGraph(context, graph, t, 240, 160, DEFAULT_PALETTE);
  assert.equal(
    calls.filter(([name]) => name === "fillRect").length,
    graph.nodes.length + 1,
  );
  assert.equal(calls.filter(([name]) => name === "stroke").length, 1);
  calls.length = 0;
  paintViewport(context, viewport, t, 240, 160, DEFAULT_PALETTE);
  assert.deepEqual(
    calls.map(([name]) => name),
    ["clearRect", "fillRect", "strokeRect"],
  );
});
