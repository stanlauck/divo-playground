// SPDX-License-Identifier: MIT OR Apache-2.0
import { LIMITS, validateViewport, type Point, type Rect } from "./model.js";
export interface Projection {
  readonly scale: number;
  readonly originX: number;
  readonly originY: number;
  readonly offsetX: number;
  readonly offsetY: number;
}
export function projection(
  bounds: Rect,
  width: number,
  height: number,
  padding = 8,
): Projection {
  validateViewport(bounds);
  if (
    ![width, height, padding].every(Number.isFinite) ||
    width < 32 ||
    height < 32 ||
    width > 2048 ||
    height > 2048 ||
    padding < 0 ||
    padding * 2 >= Math.min(width, height)
  ) {
    throw new RangeError("Invalid minimap dimensions or padding");
  }
  const scale = Math.min(
    (width - padding * 2) / bounds.width,
    (height - padding * 2) / bounds.height,
  );
  return Object.freeze({
    scale,
    originX: bounds.x,
    originY: bounds.y,
    offsetX: (width - bounds.width * scale) / 2,
    offsetY: (height - bounds.height * scale) / 2,
  });
}
export function worldToMap(p: Point, t: Projection): Point {
  return {
    x: (p.x - t.originX) * t.scale + t.offsetX,
    y: (p.y - t.originY) * t.scale + t.offsetY,
  };
}
export function mapToWorld(p: Point, t: Projection): Point {
  return {
    x: (p.x - t.offsetX) / t.scale + t.originX,
    y: (p.y - t.offsetY) / t.scale + t.originY,
  };
}
function clampPosition(value: number, extent: number): number {
  return Math.min(
    LIMITS.coordinate - extent,
    Math.max(-LIMITS.coordinate, value),
  );
}
function result(x: number, y: number, width: number, height: number): Rect {
  return validateViewport({
    x: clampPosition(x, width),
    y: clampPosition(y, height),
    width,
    height,
  });
}
export function panViewport(viewport: Rect, dx: number, dy: number): Rect {
  validateViewport(viewport);
  if (!Number.isFinite(dx) || !Number.isFinite(dy))
    throw new RangeError("Invalid pan delta");
  return result(
    viewport.x + dx,
    viewport.y + dy,
    viewport.width,
    viewport.height,
  );
}
export function centerViewport(viewport: Rect, p: Point): Rect {
  validateViewport(viewport);
  if (!Number.isFinite(p.x) || !Number.isFinite(p.y))
    throw new RangeError("Invalid center");
  return result(
    p.x - viewport.width / 2,
    p.y - viewport.height / 2,
    viewport.width,
    viewport.height,
  );
}
export function zoomViewport(
  viewport: Rect,
  factor: number,
  anchor: Point,
): Rect {
  validateViewport(viewport);
  if (
    !Number.isFinite(factor) ||
    factor <= 0 ||
    !Number.isFinite(anchor.x) ||
    !Number.isFinite(anchor.y)
  ) {
    throw new RangeError("Invalid zoom factor or anchor");
  }
  const minimum = Math.max(
    LIMITS.minExtent / viewport.width,
    LIMITS.minExtent / viewport.height,
  );
  const maximum = Math.min(
    LIMITS.maxExtent / viewport.width,
    LIMITS.maxExtent / viewport.height,
  );
  const ratio = Math.max(minimum, Math.min(maximum, factor));
  const width = Math.max(
    LIMITS.minExtent,
    Math.min(LIMITS.maxExtent, viewport.width * ratio),
  );
  const height = Math.max(
    LIMITS.minExtent,
    Math.min(LIMITS.maxExtent, viewport.height * ratio),
  );
  return result(
    anchor.x + (viewport.x - anchor.x) * ratio,
    anchor.y + (viewport.y - anchor.y) * ratio,
    width,
    height,
  );
}
export function fitViewport(viewport: Rect, bounds: Rect): Rect {
  validateViewport(viewport);
  validateViewport(bounds);
  const ratio = Math.max(
    bounds.width / viewport.width,
    bounds.height / viewport.height,
  );
  // Prefer the current aspect, but still contain the graph when that aspect
  // would require an extent beyond the supported coordinate domain.
  const resized = result(
    viewport.x,
    viewport.y,
    Math.max(
      LIMITS.minExtent,
      Math.min(LIMITS.maxExtent, viewport.width * ratio),
    ),
    Math.max(
      LIMITS.minExtent,
      Math.min(LIMITS.maxExtent, viewport.height * ratio),
    ),
  );
  return centerViewport(resized, {
    x: bounds.x + bounds.width / 2,
    y: bounds.y + bounds.height / 2,
  });
}
export function contains(viewport: Rect, p: Point): boolean {
  return (
    p.x >= viewport.x &&
    p.x <= viewport.x + viewport.width &&
    p.y >= viewport.y &&
    p.y <= viewport.y + viewport.height
  );
}
