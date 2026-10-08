// SPDX-License-Identifier: MIT OR Apache-2.0
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  GraphMinimap,
  DEFAULT_PALETTE,
  prepareGraph,
  type Rect,
  type RenderStats,
} from "../src/index.js";
import { makeGraph } from "./fixture.mjs";

const prepareStart = performance.now();
const graph = prepareGraph(makeGraph(10_000));
const prepareMs = performance.now() - prepareStart;
declare global {
  interface Window {
    pg10: {
      readonly ready: boolean;
      readonly graph: typeof graph;
      viewport: Rect;
      stats: RenderStats;
      callbacks: number;
      runBenchmark: () => Promise<object>;
      benchmark: object | null;
      setAccept: (value: boolean) => void;
      setDisabled: (value: boolean) => void;
      setViewport: (value: Rect) => void;
      unmount: () => void;
    };
  }
}
function App() {
  const [viewport, setViewport] = useState<Rect>({
    x: 100,
    y: 100,
    width: 500,
    height: 320,
  });
  const [disabled, setDisabled] = useState(false);
  const [accept, setAccept] = useState(true);
  const [result, setResult] = useState<object | null>(null);
  const stats = useRef<RenderStats>({
    graphDraws: 0,
    viewportDraws: 0,
    graphMs: 0,
    viewportMs: 0,
  });
  const callbacks = useRef(0);
  const running = useRef(false);
  const benchmark = async () => {
    if (running.current) throw new Error("Benchmark already running");
    if (
      document.visibilityState !== "visible" ||
      innerWidth <= 0 ||
      innerHeight <= 0
    )
      throw new Error("Benchmark requires a visible nonzero viewport");
    running.current = true;
    try {
      const nextFrame = () =>
        new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => {
            cancelAnimationFrame(frame);
            reject(
              new Error("No browser frame for two seconds; benchmark invalid"),
            );
          }, 2000);
          const frame = requestAnimationFrame(() => {
            clearTimeout(timeout);
            if (document.visibilityState !== "visible") {
              reject(new Error("Tab became hidden; benchmark invalid"));
              return;
            }
            resolve();
          });
        });
      const gaps: number[] = [],
        costs: number[] = [];
      const before = stats.current.graphDraws;
      const overlayBefore = stats.current.viewportDraws;
      const initialGraphPaintMs = stats.current.graphMs;
      let last = performance.now();
      for (let i = 0; i < 390; i++) {
        await nextFrame();
        const now = performance.now();
        if (i >= 30) {
          gaps.push(now - last);
          costs.push(stats.current.viewportMs);
        }
        last = now;
        setViewport({
          x: 100 + Math.sin(i / 30) * 700,
          y: 100 + Math.cos(i / 50) * 300,
          width: 500 + Math.sin(i / 40) * 100,
          height: 320 + Math.sin(i / 40) * 64,
        });
      }
      await nextFrame();
      await nextFrame();
      const ordered = [...gaps].sort((a, b) => a - b);
      const report = {
        version: 1,
        nodes: graph.nodes.length,
        edges: graph.edges.length,
        frames: gaps.length,
        scenario: "controlled viewport pan/zoom, static cached graph",
        durationMs: gaps.reduce((a, b) => a + b, 0),
        fps: 1000 / (gaps.reduce((a, b) => a + b, 0) / gaps.length),
        frameP50Ms: ordered[Math.floor(ordered.length * 0.5)],
        frameP95Ms: ordered[Math.floor(ordered.length * 0.95)],
        viewportPaintMeanMs: costs.reduce((a, b) => a + b, 0) / costs.length,
        graphDrawsDuringRun: stats.current.graphDraws - before,
        viewportDrawsDuringRun: stats.current.viewportDraws - overlayBefore,
        prepareMs,
        initialGraphPaintMs,
        userAgent: navigator.userAgent,
        dpr: devicePixelRatio,
        visible: document.visibilityState,
        hardwareConcurrency: navigator.hardwareConcurrency,
        minimap: { width: 320, height: 200 },
        note: "Local RAF cadence, not a portable FPS guarantee or GPU timing. Static graph only; initial preparation and graph paint measured separately.",
      };
      window.pg10.benchmark = report;
      setResult(report);
      return report;
    } finally {
      running.current = false;
    }
  };
  useEffect(() => {
    window.pg10 = {
      ready: true,
      graph,
      viewport,
      stats: stats.current,
      callbacks: callbacks.current,
      runBenchmark: benchmark,
      benchmark: result,
      setAccept,
      setDisabled,
      setViewport,
      unmount: () => root.unmount(),
    };
  });
  const onRender = (value: RenderStats) => {
    stats.current = value;
    if (window.pg10) window.pg10.stats = value;
  };
  return (
    <>
      <h1>Synthetic graph minimap</h1>
      <p>
        10,000 invented rectangular nodes and {graph.edges.length} edges. No
        network assets.
      </p>
      <GraphMinimap
        graph={graph}
        viewport={viewport}
        width={320}
        height={200}
        disabled={disabled}
        palette={{ ...DEFAULT_PALETTE }}
        onViewportChange={(next) => {
          callbacks.current++;
          if (window.pg10) window.pg10.callbacks = callbacks.current;
          if (accept) setViewport(next);
        }}
        onRender={onRender}
      />
      <p id="viewport">Viewport: {JSON.stringify(viewport)}</p>
      <p>
        <button
          onClick={() => {
            void benchmark().catch((error: Error) =>
              setResult({ error: error.message }),
            );
          }}
        >
          Run 360-frame benchmark
        </button>
      </p>
      <pre id="benchmark">
        {result ? JSON.stringify(result, null, 2) : "Benchmark not run."}
      </pre>
    </>
  );
}
const root = createRoot(document.getElementById("root")!);
root.render(<App />);
