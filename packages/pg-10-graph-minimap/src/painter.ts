// SPDX-License-Identifier: MIT OR Apache-2.0
import { worldToMap, type Projection } from "./geometry.js";
import { type PreparedGraph, type Rect } from "./model.js";
export interface Palette {
  readonly background: string;
  readonly nodes: string;
  readonly edges: string;
  readonly viewport: string;
  readonly viewportFill: string;
}
export const DEFAULT_PALETTE: Palette = Object.freeze({
  background: "#f8fafc",
  nodes: "#475569",
  edges: "#94a3b8",
  viewport: "#1d4ed8",
  viewportFill: "#2563eb22",
});
export function setupCanvas(
  canvas: HTMLCanvasElement,
  width: number,
  height: number,
  dpr: number,
): CanvasRenderingContext2D | null {
  const ratio = Math.max(
    1,
    Math.min(
      3,
      Number.isFinite(dpr) ? dpr : 1,
      Math.sqrt(4_194_304 / (width * height)),
    ),
  );
  const pixelWidth = Math.floor(width * ratio),
    pixelHeight = Math.floor(height * ratio);
  if (canvas.width !== pixelWidth) canvas.width = pixelWidth;
  if (canvas.height !== pixelHeight) canvas.height = pixelHeight;
  const context = canvas.getContext("2d");
  context?.setTransform(pixelWidth / width, 0, 0, pixelHeight / height, 0, 0);
  return context;
}
export function paintGraph(
  context: CanvasRenderingContext2D,
  graph: PreparedGraph,
  t: Projection,
  width: number,
  height: number,
  colors: Palette,
): void {
  context.clearRect(0, 0, width, height);
  context.fillStyle = colors.background;
  context.fillRect(0, 0, width, height);
  context.strokeStyle = colors.edges;
  context.lineWidth = 1;
  context.beginPath();
  for (const edge of graph.edges) {
    const first = worldToMap(edge.points[0]!, t);
    context.moveTo(first.x, first.y);
    for (let i = 1; i < edge.points.length; i++) {
      const p = worldToMap(edge.points[i]!, t);
      context.lineTo(p.x, p.y);
    }
  }
  context.stroke();
  context.fillStyle = colors.nodes;
  for (const node of graph.nodes) {
    const p = worldToMap(node, t);
    context.fillRect(
      p.x,
      p.y,
      Math.max(1, node.width * t.scale),
      Math.max(1, node.height * t.scale),
    );
  }
}
export function paintViewport(
  context: CanvasRenderingContext2D,
  viewport: Rect,
  t: Projection,
  width: number,
  height: number,
  colors: Palette,
): void {
  context.clearRect(0, 0, width, height);
  const p = worldToMap(viewport, t);
  context.fillStyle = colors.viewportFill;
  context.strokeStyle = colors.viewport;
  context.lineWidth = 2;
  context.fillRect(
    p.x,
    p.y,
    viewport.width * t.scale,
    viewport.height * t.scale,
  );
  context.strokeRect(
    p.x,
    p.y,
    viewport.width * t.scale,
    viewport.height * t.scale,
  );
}
