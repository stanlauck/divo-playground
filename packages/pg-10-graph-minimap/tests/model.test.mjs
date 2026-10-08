// SPDX-License-Identifier: MIT OR Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";
import {
  prepareGraph,
  prepareDocument,
  validateViewport,
  GraphError,
  LIMITS,
} from "../dist/src/model.js";
import { makeDocument, makeGraph } from "../examples/fixture.mjs";
const rejects = (value, code) =>
  assert.throws(
    () => prepareGraph(value),
    (e) =>
      e instanceof GraphError &&
      e.code === code &&
      !e.message.includes("secret"),
  );
test("synthetic document has resolved straight and complete routed edges", () => {
  const doc = makeDocument();
  const { graph, viewport } = prepareDocument(doc);
  assert.deepEqual(viewport, doc.viewport);
  assert.equal(graph.edges[0].points.length, 2);
  assert.deepEqual(graph.edges.at(-1).points, doc.edges.at(-1).points);
  assert.deepEqual(graph.edges[0].points[0], {
    x: doc.nodes[0].x + 10,
    y: doc.nodes[0].y + 6,
  });
});
test("caller data and nested routes are detached and frozen", () => {
  const input = makeGraph(2),
    graph = prepareGraph(input);
  input.nodes[0].x = 123;
  input.edges.at(-1).points[0].x = 123;
  assert.notEqual(graph.nodes[0].x, 123);
  assert.notEqual(graph.edges.at(-1).points[0].x, 123);
  for (const value of [
    graph,
    graph.bounds,
    graph.nodes,
    graph.nodes[0],
    graph.edges,
    graph.edges.at(-1).points,
    graph.edges.at(-1).points[0],
  ]) {
    assert.ok(Object.isFrozen(value));
  }
});
test("empty graph uses finite neutral unit bounds", () => {
  assert.deepEqual(prepareGraph({ version: 1, nodes: [], edges: [] }).bounds, {
    x: -0.5,
    y: -0.5,
    width: 1,
    height: 1,
  });
});
test("negative bounds include full nodes and routed excursions", () => {
  const input = makeGraph(2);
  input.edges.at(-1).points[1] = { x: -1000, y: 500 };
  const bounds = prepareGraph(input).bounds;
  assert.equal(bounds.x, -1000);
  assert.equal(bounds.y + bounds.height, 500);
});
for (const value of [null, [], true, 3, "secret", new Map(), new Date()]) {
  test(`reject root type ${String(value)}`, () => rejects(value, "object"));
}
for (const version of [0, 2, "1", true, null]) {
  test(`reject version ${version}`, () =>
    rejects({ ...makeGraph(), version }, "version"));
}
test("unknown fields are rejected, not interpreted", () =>
  rejects({ ...makeGraph(), script: "secret" }, "fields"));
test("node and edge IDs have separate unique domains", () => {
  const input = makeGraph(2);
  input.edges[0].id = input.nodes[0].id;
  assert.doesNotThrow(() => prepareGraph(input));
  input.nodes[1].id = input.nodes[0].id;
  rejects(input, "duplicate");
});
test("duplicate edge and missing endpoints are rejected", () => {
  const input = makeGraph(2);
  input.edges[1].id = input.edges[0].id;
  rejects(input, "duplicate");
  input.edges[1].id = "other";
  input.edges[0].target = "absent";
  rejects(input, "reference");
});
for (const id of ["", " spaced ", "control\t", "a".repeat(129), "\ud800"]) {
  test("invalid ID is rejected safely", () => {
    const input = makeGraph(1);
    input.nodes[0].id = id;
    rejects(input, "id");
  });
}
test("Unicode scalar IDs remain case sensitive", () => {
  const input = makeGraph(2);
  input.nodes[0].id = "🎬";
  input.nodes[1].id = "A";
  input.edges = [];
  assert.equal(prepareGraph(input).nodes[0].id, "🎬");
});
for (const value of [NaN, Infinity, -Infinity, "1", LIMITS.coordinate + 1]) {
  test("invalid coordinate", () => {
    const input = makeGraph(1);
    input.nodes[0].x = value;
    rejects(input, "number");
  });
}
for (const value of [0, -1, NaN, Infinity, "1", 1e-12]) {
  test("invalid extent", () => {
    const input = makeGraph(1);
    input.nodes[0].width = value;
    rejects(input, "rectangle");
  });
}
test("rectangle end coordinates must be representable and bounded", () => {
  const input = makeGraph(1);
  input.nodes[0].x = LIMITS.coordinate;
  rejects(input, "rectangle");
});
for (const points of [
  null,
  [],
  [{ x: 0, y: 0 }],
  [
    { x: NaN, y: 0 },
    { x: 1, y: 1 },
  ],
]) {
  test("invalid route", () => {
    const input = makeGraph(2);
    input.edges[0].points = points;
    assert.throws(() => prepareGraph(input), GraphError);
  });
}
test("IDs/control data cannot execute getters or toJSON", () => {
  let called = 0;
  const input = makeGraph(1);
  Object.defineProperty(input.nodes[0], "x", {
    enumerable: true,
    get() {
      called++;
      return 0;
    },
  });
  rejects(input, "fields");
  assert.equal(called, 0);
  const value = {
    ...makeGraph(1),
    toJSON() {
      called++;
      return makeGraph(1);
    },
  };
  rejects(value, "fields");
  assert.equal(called, 0);
});
test("sparse, accessor, extra-property, and inherited arrays are rejected", () => {
  for (const mutation of [
    (a) => {
      delete a[0];
    },
    (a) => {
      Object.defineProperty(a, "0", {
        get() {
          throw new Error("must not run");
        },
        enumerable: true,
      });
    },
    (a) => {
      a.extra = 1;
    },
    (a) => {
      Object.setPrototypeOf(a, null);
    },
  ]) {
    const input = makeGraph(1);
    mutation(input.nodes);
    rejects(input, "array");
  }
});
test("collection budgets are enforced before traversal", () => {
  rejects({ version: 1, nodes: Array(LIMITS.nodes + 1), edges: [] }, "limit");
  rejects({ version: 1, nodes: [], edges: Array(LIMITS.edges + 1) }, "limit");
  const input = makeGraph(2);
  input.edges[0].points = Array(LIMITS.pointsPerEdge + 1);
  rejects(input, "limit");
});
test("10000-node fixture prepares without mutation", () => {
  const input = makeGraph(10_000);
  const graph = prepareGraph(input);
  assert.equal(graph.nodes.length, 10_000);
  assert.equal(graph.edges.length, 19_801);
  assert.equal(input.nodes[0].x, -100);
});
test("strict viewport and full document root", () => {
  assert.throws(
    () => validateViewport({ x: 0, y: 0, width: 10, height: 5, other: true }),
    GraphError,
  );
  assert.throws(
    () => prepareDocument({ ...makeDocument(), version: 2 }),
    GraphError,
  );
});
