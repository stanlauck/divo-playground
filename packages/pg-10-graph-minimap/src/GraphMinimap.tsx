// SPDX-License-Identifier: MIT OR Apache-2.0
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import {
  centerViewport,
  contains,
  fitViewport,
  mapToWorld,
  panViewport,
  projection,
  zoomViewport,
} from "./geometry.js";
import {
  requirePreparedGraph,
  validateViewport,
  type Point,
  type PreparedGraph,
  type Rect,
} from "./model.js";
import {
  DEFAULT_PALETTE,
  paintGraph,
  paintViewport,
  setupCanvas,
  type Palette,
} from "./painter.js";

export interface RenderStats {
  readonly graphDraws: number;
  readonly viewportDraws: number;
  readonly graphMs: number;
  readonly viewportMs: number;
}
export interface ViewportChange {
  readonly reason: "pan" | "zoom" | "fit" | "keyboard";
}
export interface GraphMinimapProps {
  readonly graph: PreparedGraph;
  readonly viewport: Rect;
  readonly onViewportChange: (viewport: Rect, change: ViewportChange) => void;
  readonly width?: number;
  readonly height?: number;
  readonly padding?: number;
  readonly palette?: Palette;
  readonly label?: string;
  readonly disabled?: boolean;
  readonly onRender?: (stats: RenderStats) => void;
}
interface Drag {
  readonly id: number;
  readonly start: Point;
  readonly viewport: Rect;
}
function watchRatio(schedule: () => void): () => void {
  let ratio = window.devicePixelRatio;
  let media = window.matchMedia(`(resolution: ${ratio}dppx)`);
  const change = () => {
    if (window.devicePixelRatio === ratio) return;
    ratio = window.devicePixelRatio;
    media.removeEventListener("change", change);
    media = window.matchMedia(`(resolution: ${ratio}dppx)`);
    media.addEventListener("change", change);
    schedule();
  };
  media.addEventListener("change", change);
  // Some browsers defer resolution-only media changes until a resize.
  window.addEventListener("resize", change);
  window.visualViewport?.addEventListener("resize", change);
  return () => {
    media.removeEventListener("change", change);
    window.removeEventListener("resize", change);
    window.visualViewport?.removeEventListener("resize", change);
  };
}
export function GraphMinimap({
  graph,
  viewport: rawViewport,
  onViewportChange,
  width = 240,
  height = 160,
  padding = 8,
  palette = DEFAULT_PALETTE,
  label = "Graph minimap",
  disabled = false,
  onRender,
}: GraphMinimapProps) {
  requirePreparedGraph(graph);
  const viewport = validateViewport(rawViewport);
  const suppliedColors = (() => {
    const keys = [
      "background",
      "nodes",
      "edges",
      "viewport",
      "viewportFill",
    ] as const;
    if (
      !palette ||
      Object.getPrototypeOf(palette) !== Object.prototype ||
      Reflect.ownKeys(palette).length !== keys.length
    ) {
      throw new RangeError("Palette requires exactly five hex colors");
    }
    const values = {} as { -readonly [K in keyof Palette]: Palette[K] };
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(palette, key);
      if (
        !descriptor ||
        !("value" in descriptor) ||
        typeof descriptor.value !== "string" ||
        !/^#[a-f\d]{6}(?:[a-f\d]{2})?$/i.test(descriptor.value)
      ) {
        throw new RangeError("Palette requires six- or eight-digit hex colors");
      }
      values[key] = descriptor.value;
    }
    return values;
  })();
  const {
    background,
    nodes,
    edges,
    viewport: viewportColor,
    viewportFill,
  } = suppliedColors;
  const colors = useMemo(
    () => ({
      background,
      nodes,
      edges,
      viewport: viewportColor,
      viewportFill,
    }),
    [background, nodes, edges, viewportColor, viewportFill],
  );
  const t = useMemo(
    () => projection(graph.bounds, width, height, padding),
    [graph, width, height, padding],
  );
  const base = useRef<HTMLCanvasElement>(null),
    overlay = useRef<HTMLCanvasElement>(null);
  const surface = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const stats = useRef({
    graphDraws: 0,
    viewportDraws: 0,
    graphMs: 0,
    viewportMs: 0,
  });
  const report = useRef(onRender);
  const live = useRef({
    viewport,
    onViewportChange,
    disabled,
    t,
    width,
    height,
  });
  const instructionId = useId();
  useEffect(() => {
    report.current = onRender;
  });
  useEffect(() => {
    live.current = { viewport, onViewportChange, disabled, t, width, height };
  });

  useEffect(() => {
    let frame = 0;
    const draw = () => {
      const canvas = base.current;
      if (!canvas) return;
      const context = setupCanvas(
        canvas,
        width,
        height,
        window.devicePixelRatio || 1,
      );
      if (!context) return;
      const start = performance.now();
      paintGraph(context, graph, t, width, height, colors);
      stats.current.graphDraws++;
      stats.current.graphMs = performance.now() - start;
      report.current?.({ ...stats.current });
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(draw);
    };
    schedule();
    // Browser zoom / DPR changes invalidate both backing stores.
    const unwatch = watchRatio(schedule);
    return () => {
      cancelAnimationFrame(frame);
      unwatch();
    };
  }, [graph, t, width, height, colors]);

  useEffect(() => {
    let frame = 0;
    const draw = () => {
      const canvas = overlay.current;
      if (!canvas) return;
      const context = setupCanvas(
        canvas,
        width,
        height,
        window.devicePixelRatio || 1,
      );
      if (!context) return;
      const start = performance.now();
      paintViewport(context, viewport, t, width, height, colors);
      stats.current.viewportDraws++;
      stats.current.viewportMs = performance.now() - start;
      report.current?.({ ...stats.current });
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(draw);
    };
    schedule();
    const unwatch = watchRatio(schedule);
    return () => {
      cancelAnimationFrame(frame);
      unwatch();
    };
  }, [
    viewport.x,
    viewport.y,
    viewport.width,
    viewport.height,
    t,
    width,
    height,
    colors,
  ]);

  useEffect(() => {
    const element = surface.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      const current = live.current;
      if (
        current.disabled ||
        !Number.isFinite(event.deltaY) ||
        event.deltaY === 0
      )
        return;
      const box = element.getBoundingClientRect();
      if (box.width <= 0 || box.height <= 0) return;
      event.preventDefault();
      const p = mapToWorld(
        {
          x: ((event.clientX - box.left) * current.width) / box.width,
          y: ((event.clientY - box.top) * current.height) / box.height,
        },
        current.t,
      );
      const unit =
        event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? current.height : 1;
      const factor = Math.exp(
        Math.max(-1, Math.min(1, event.deltaY * unit * 0.002)),
      );
      current.onViewportChange(zoomViewport(current.viewport, factor, p), {
        reason: "zoom",
      });
      drag.current = null;
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, []);
  useEffect(() => {
    if (disabled) drag.current = null;
  }, [disabled]);
  useEffect(() => {
    drag.current = null;
  }, [t]);
  useEffect(
    () => () => {
      drag.current = null;
    },
    [],
  );

  const pointerPoint = (event: PointerEvent<HTMLDivElement>): Point | null => {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) return null;
    return mapToWorld(
      {
        x: ((event.clientX - box.left) * width) / box.width,
        y: ((event.clientY - box.top) * height) / box.height,
      },
      t,
    );
  };
  const release = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.id !== event.pointerId) return;
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };
  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0 || !event.isPrimary) return;
    const point = pointerPoint(event);
    if (!point) return;
    event.preventDefault();
    event.currentTarget.focus();
    const initial = contains(viewport, point)
      ? viewport
      : centerViewport(viewport, point);
    drag.current = { id: event.pointerId, start: point, viewport: initial };
    event.currentTarget.setPointerCapture(event.pointerId);
    if (initial !== viewport) onViewportChange(initial, { reason: "pan" });
  };
  const pointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (disabled || !current || current.id !== event.pointerId) return;
    const point = pointerPoint(event);
    if (point)
      onViewportChange(
        panViewport(
          current.viewport,
          point.x - current.start.x,
          point.y - current.start.y,
        ),
        { reason: "pan" },
      );
  };
  const zoom = (factor: number, reason: ViewportChange["reason"] = "zoom") => {
    onViewportChange(
      zoomViewport(viewport, factor, {
        x: viewport.x + viewport.width / 2,
        y: viewport.y + viewport.height / 2,
      }),
      { reason },
    );
  };
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (disabled || event.target !== event.currentTarget) return;
    const step = event.shiftKey ? 0.5 : 0.1;
    const directions: Record<string, readonly [number, number]> = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    const direction = Object.hasOwn(directions, event.key)
      ? directions[event.key]
      : undefined;
    if (direction) {
      event.preventDefault();
      onViewportChange(
        panViewport(
          viewport,
          direction[0] * viewport.width * step,
          direction[1] * viewport.height * step,
        ),
        { reason: "keyboard" },
      );
    } else if (event.key === "+" || event.key === "=" || event.key === "-") {
      event.preventDefault();
      zoom(event.key === "-" ? 1.25 : 0.8, "keyboard");
    } else if (event.key === "Home") {
      event.preventDefault();
      onViewportChange(fitViewport(viewport, graph.bounds), { reason: "fit" });
    }
  };
  const canvasStyle = {
    position: "absolute" as const,
    inset: 0,
    width,
    height,
    pointerEvents: "none" as const,
  };
  return (
    <section aria-label={label} style={{ width }}>
      <div
        ref={surface}
        role="group"
        aria-label={`${label} pan and zoom`}
        aria-describedby={instructionId}
        aria-disabled={disabled}
        tabIndex={disabled ? -1 : 0}
        style={{
          position: "relative",
          width,
          height,
          touchAction: disabled ? "auto" : "none",
          cursor: disabled ? "default" : "grab",
        }}
        onPointerDown={pointerDown}
        onPointerMove={pointerMove}
        onPointerUp={release}
        onPointerCancel={release}
        onLostPointerCapture={() => {
          drag.current = null;
        }}
        onKeyDown={keyDown}
      >
        <canvas ref={base} aria-hidden="true" style={canvasStyle} />
        <canvas ref={overlay} aria-hidden="true" style={canvasStyle} />
      </div>
      <div style={{ display: "flex", gap: 4, marginTop: 4 }}>
        <button
          type="button"
          disabled={disabled}
          onClick={() => zoom(0.8)}
          aria-label="Zoom in"
        >
          +
        </button>
        <button
          type="button"
          disabled={disabled}
          onClick={() => zoom(1.25)}
          aria-label="Zoom out"
        >
          −
        </button>
        <button
          type="button"
          disabled={disabled}
          onClick={() =>
            onViewportChange(fitViewport(viewport, graph.bounds), {
              reason: "fit",
            })
          }
        >
          Fit graph
        </button>
      </div>
      <p id={instructionId} style={{ fontSize: 12, margin: "4px 0" }}>
        {graph.nodes.length} nodes, {graph.edges.length} edges. Click or drag to
        pan; scroll to zoom. Keyboard: arrows, +/−, Home to fit.
      </p>
    </section>
  );
}
