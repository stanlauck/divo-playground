// SPDX-License-Identifier: MIT OR Apache-2.0
export interface Point {
  readonly x: number;
  readonly y: number;
}
export interface Rect extends Point {
  readonly width: number;
  readonly height: number;
}
export interface GraphNode extends Rect {
  readonly id: string;
}
export interface GraphEdge {
  readonly id: string;
  readonly source: string;
  readonly target: string;
  /** Complete polyline in world coordinates, including its endpoints. */
  readonly points?: readonly Point[];
}
export interface GraphInput {
  readonly version: 1;
  readonly nodes: readonly GraphNode[];
  readonly edges: readonly GraphEdge[];
}
export interface GraphDocument extends GraphInput {
  readonly viewport: Rect;
}
export interface PreparedEdge extends Omit<GraphEdge, "points"> {
  readonly points: readonly Point[];
}
export interface PreparedGraph {
  readonly version: 1;
  readonly nodes: readonly GraphNode[];
  readonly edges: readonly PreparedEdge[];
  readonly bounds: Rect;
}
export type ModelErrorCode =
  | "object"
  | "fields"
  | "version"
  | "array"
  | "limit"
  | "id"
  | "duplicate"
  | "number"
  | "rectangle"
  | "reference"
  | "points"
  | "prepared_graph";
export class GraphError extends Error {
  readonly code: ModelErrorCode;
  readonly path: string;
  constructor(code: ModelErrorCode, path: string) {
    super(`Invalid graph: ${code} at ${path}`);
    this.name = "GraphError";
    this.code = code;
    this.path = path;
  }
}
export const LIMITS = Object.freeze({
  nodes: 100_000,
  edges: 200_000,
  points: 1_000_000,
  pointsPerEdge: 10_000,
  coordinate: 1_000_000_000,
  minExtent: 0.000001,
  maxExtent: 2_000_000_000,
  idBytes: 128,
});
const prepared = new WeakSet<object>();
const encoder = new TextEncoder();

function fail(code: ModelErrorCode, path: string): never {
  throw new GraphError(code, path);
}
function object(
  value: unknown,
  required: readonly string[],
  optional: readonly string[],
  path: string,
): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    fail("object", path);
  const proto: unknown = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== null) fail("object", path);
  const result: Record<string, unknown> = Object.create(null) as Record<
    string,
    unknown
  >;
  for (const key of Reflect.ownKeys(value)) {
    if (
      typeof key !== "string" ||
      (!required.includes(key) && !optional.includes(key))
    )
      fail("fields", path);
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (!descriptor || !("value" in descriptor) || !descriptor.enumerable)
      fail("fields", path);
    result[key] = descriptor.value as unknown;
  }
  for (const key of required) if (!(key in result)) fail("fields", path);
  return result;
}
function array(
  value: unknown,
  limit: number,
  path: string,
): readonly unknown[] {
  if (!Array.isArray(value)) fail("array", path);
  if (value.length > limit) fail("limit", path);
  // Only ordinary dense data arrays, not exotic prototypes or accessors.
  if (Object.getPrototypeOf(value) !== Array.prototype) fail("array", path);
  if (Reflect.ownKeys(value).length !== value.length + 1) fail("array", path);
  const result: unknown[] = [];
  for (let i = 0; i < value.length; i++) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(i));
    if (!descriptor || !("value" in descriptor) || !descriptor.enumerable)
      fail("array", path);
    result.push(descriptor.value as unknown);
  }
  return result;
}
function id(value: unknown, path: string): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > LIMITS.idBytes ||
    value.trim() !== value ||
    /[\u0000-\u001f\u007f-\u009f\ud800-\udfff]/u.test(value) ||
    encoder.encode(value).length > LIMITS.idBytes
  )
    fail("id", path);
  return value;
}
function number(value: unknown, path: string): number {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    Math.abs(value) > LIMITS.coordinate
  )
    fail("number", path);
  return value;
}
function point(value: unknown, path: string): Point {
  const record = object(value, ["x", "y"], [], path);
  return Object.freeze({
    x: number(record.x, path),
    y: number(record.y, path),
  });
}
function rectangle(record: Record<string, unknown>, path: string): Rect {
  const x = number(record.x, path);
  const y = number(record.y, path);
  const width = record.width;
  const height = record.height;
  if (
    typeof width !== "number" ||
    typeof height !== "number" ||
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    width < LIMITS.minExtent ||
    height < LIMITS.minExtent ||
    width > LIMITS.maxExtent ||
    height > LIMITS.maxExtent ||
    x + width <= x ||
    y + height <= y ||
    Math.abs(x + width) > LIMITS.coordinate ||
    Math.abs(y + height) > LIMITS.coordinate
  )
    fail("rectangle", path);
  return Object.freeze({ x, y, width, height });
}
export function validateViewport(value: unknown): Rect {
  return rectangle(
    object(value, ["x", "y", "width", "height"], [], "$.viewport"),
    "$.viewport",
  );
}
export function requirePreparedGraph(value: PreparedGraph): void {
  if (!prepared.has(value)) fail("prepared_graph", "$.graph");
}
export function prepareGraph(value: unknown): PreparedGraph {
  const root = object(value, ["version", "nodes", "edges"], [], "$");
  if (root.version !== 1) fail("version", "$.version");
  const nodes: GraphNode[] = [];
  const byId = new Map<string, GraphNode>();
  let left = Infinity,
    top = Infinity,
    right = -Infinity,
    bottom = -Infinity;
  const include = (x: number, y: number): void => {
    left = Math.min(left, x);
    top = Math.min(top, y);
    right = Math.max(right, x);
    bottom = Math.max(bottom, y);
  };
  const rawNodes = array(root.nodes, LIMITS.nodes, "$.nodes");
  for (let i = 0; i < rawNodes.length; i++) {
    const path = `$.nodes[${i}]`;
    const record = object(
      rawNodes[i],
      ["id", "x", "y", "width", "height"],
      [],
      path,
    );
    const name = id(record.id, path);
    if (byId.has(name)) fail("duplicate", path);
    const rect = rectangle(record, path);
    const node = Object.freeze({ id: name, ...rect });
    nodes.push(node);
    byId.set(name, node);
    include(node.x, node.y);
    include(node.x + node.width, node.y + node.height);
  }
  const edges: PreparedEdge[] = [];
  const seen = new Set<string>();
  let totalPoints = 0;
  const rawEdges = array(root.edges, LIMITS.edges, "$.edges");
  for (let i = 0; i < rawEdges.length; i++) {
    const path = `$.edges[${i}]`;
    const record = object(
      rawEdges[i],
      ["id", "source", "target"],
      ["points"],
      path,
    );
    const name = id(record.id, path);
    if (seen.has(name)) fail("duplicate", path);
    seen.add(name);
    const source = id(record.source, path),
      target = id(record.target, path);
    const a = byId.get(source),
      b = byId.get(target);
    if (!a || !b) fail("reference", path);
    let points: readonly Point[];
    if ("points" in record) {
      const raw = array(record.points, LIMITS.pointsPerEdge, path);
      if (raw.length < 2) fail("points", path);
      points = Object.freeze(raw.map((p) => point(p, path)));
    } else {
      points = Object.freeze([
        Object.freeze({ x: a.x + a.width / 2, y: a.y + a.height / 2 }),
        Object.freeze({ x: b.x + b.width / 2, y: b.y + b.height / 2 }),
      ]);
    }
    totalPoints += points.length;
    if (totalPoints > LIMITS.points) fail("limit", path);
    for (const p of points) include(p.x, p.y);
    edges.push(Object.freeze({ id: name, source, target, points }));
  }
  const bounds: Rect =
    nodes.length === 0
      ? Object.freeze({ x: -0.5, y: -0.5, width: 1, height: 1 })
      : Object.freeze({
          x: left,
          y: top,
          width: Math.max(LIMITS.minExtent, right - left),
          height: Math.max(LIMITS.minExtent, bottom - top),
        });
  const graph = Object.freeze({
    version: 1 as const,
    nodes: Object.freeze(nodes),
    edges: Object.freeze(edges),
    bounds,
  });
  prepared.add(graph);
  return graph;
}
export function prepareDocument(
  value: unknown,
): Readonly<{ graph: PreparedGraph; viewport: Rect }> {
  const root = object(
    value,
    ["version", "nodes", "edges", "viewport"],
    [],
    "$",
  );
  const viewport = validateViewport(root.viewport);
  const graph = prepareGraph({
    version: root.version,
    nodes: root.nodes,
    edges: root.edges,
  });
  return Object.freeze({ graph, viewport });
}
