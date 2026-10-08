# PG-10: controlled graph minimap

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

A standalone TS/React Canvas minimap for rectangular nodes, straight/polyline
edges, and a controlled world-space viewport. No graph layout engine, renderer
for the main graph, remote assets, networking, or host-application dependency.

React 18.3+ or 19 is a peer dependency. The included production demo is built
with pinned React 19.3.0. Developer tools require Node 22+.

## API

```tsx
import { useMemo, useState } from "react";
import {
  GraphMinimap,
  prepareGraph,
} from "@divo-playground/pg-10-graph-minimap";

const graph = useMemo(
  () =>
    prepareGraph({
      version: 1,
      nodes: [
        { id: "a", x: -40, y: 20, width: 30, height: 18 },
        { id: "b", x: 80, y: 20, width: 30, height: 18 },
      ],
      edges: [{ id: "ab", source: "a", target: "b" }],
    }),
  [],
);
const [viewport, setViewport] = useState({
  x: -60,
  y: 0,
  width: 180,
  height: 100,
});

<GraphMinimap
  graph={graph}
  viewport={viewport}
  onViewportChange={setViewport}
  width={240}
  height={160}
  label="Graph overview"
/>;
```

`prepareGraph()` validates and copies caller data into a deeply frozen snapshot.
Keep that snapshot stable across viewport changes. Replace it when the graph
changes; mutating the original input does not change the prepared graph.
Only snapshots produced by this package are accepted by the component, not
deserialized or fabricated `PreparedGraph` lookalikes.

`viewport` is `{x,y,width,height}` in the same world coordinates. It is always
controlled: interactions emit `onViewportChange(next, {reason})`, and the
parent decides whether/when to apply the suggestion. A rejected suggestion
does not move the displayed viewport. There is no optimistic camera state.
Reasons are `pan`, `zoom`, `keyboard`, and `fit`.

Other props: `width`/`height` (CSS pixels), `padding`, five-color `palette`,
accessible `label`, `disabled`, and optional `onRender(stats)`.
Palette colors must be six/eight-digit hex. CSS styling of controls is left to
the consumer. The example is intentionally minimal, not a theme engine.

### Controls

- Click outside the viewport to center it; drag inside it to pan without a jump.
  Click-and-drag outside starts from the centered viewport.
- Primary mouse/touch/pen pointer capture supports dragging beyond the surface.
  Cancellation, lost capture, disable and projection changes end the drag.
- Wheel zoom anchors the world point under the pointer and consumes scrolling
  only on the enabled minimap. Pixel, line and page wheel delta modes are handled.
- Arrow keys pan by 10% of the viewport (Shift: 50%); `+`/`-` zoom; Home fits.
  Named Zoom in/out and Fit graph buttons provide a keyboard alternative.
- Horizontal-only wheel gestures pass through without a zoom callback.
- `disabled` prevents gestures/callbacks and disables the buttons while allowing
  native touch and wheel page scrolling.

Zoom preserves viewport aspect ratio, except floating-point rounding.
Fit prefers the current aspect but relaxes it if the finite extent limit would
otherwise exclude part of the graph. Pan is **not clamped to graph bounds**. Suggestions are
clamped only to the documented finite coordinate domain. The viewport may lie
partly or entirely outside the fixed graph projection and is canvas-clipped.
Empty graphs have neutral unit bounds. Self-edges without a route degenerate to
a point; provide a polyline to show a self-loop.

Geometry helpers (`projection`, world/map conversion, pan/center/zoom/fit) are
also exported. Projection stores a world origin, scale and map-space letterbox
offsets. World/map conversion subtracts the origin before scaling to avoid
cancelling huge screen values for tiny shapes near coordinate limits.
Projection fits graph bounds with centered letterboxing; nodes
remain rectangles, never a DOM element per node. Tiny node marks have a minimum
one-CSS-pixel width/height for visibility.

## JSON v1

[`schema.json`](schema.json) and [`samples/synthetic.json`](samples/synthetic.json)
describe a document with exactly `version`, `nodes`, `edges`, and `viewport`.
`prepareDocument(value)` returns `{graph, viewport}`; `prepareGraph(value)`
accepts only the three graph fields without the viewport.

- Nodes: `{id,x,y,width,height}`.
- Edges: `{id,source,target,points?}`. Endpoints refer to existing node IDs.
  No route means a straight line between node centers. `points`, when supplied,
  is the **complete polyline**, including first and last endpoints. It is not
  a list of intermediate bends, and endpoints are not automatically inserted.
- Viewport: `{x,y,width,height}`, with strictly positive extents.

Node and edge IDs have separate uniqueness domains. IDs are case-sensitive,
nonempty, trimmed, control/surrogate-free Unicode strings of at most 128 UTF-8
bytes. Unknown fields, sparse/exotic arrays, accessor properties and non-plain
records are rejected. Ordinary JSON-shaped data is expected; accessor/toJSON
hooks are not used for copying. This is a model validator, not a custom JSON
text decoder (standard `JSON.parse` behavior applies before it is called).

`GraphError` has a safe `code` and structural `path`; its message never contains
input text or IDs. The JSON Schema checks shape. Runtime additionally checks
byte limits, finite rectangle ends, uniqueness, references and total routes.

| Limit                                                                 | Value                                           |
| --------------------------------------------------------------------- | ----------------------------------------------- |
| Nodes / edges                                                         | 100,000 / 200,000                               |
| Points per edge / total points (including derived straight endpoints) | 10,000 / 1,000,000                              |
| Coordinate and rectangle-end magnitude                                | 1,000,000,000                                   |
| Width/height                                                          | 0.000001…2,000,000,000                          |
| Minimap CSS size                                                      | 32…2048 pixels per dimension                    |
| Backing store                                                         | DPR capped at 3 and 4,194,304 pixels per canvas |

The coordinate domain prevents overflowing Canvas transforms. Input is
materialized, not streamed. Limits are not a guarantee that every maximum-size
graph prepares/renders quickly. Keep snapshots stable and benchmark your graph.

## Caching and performance

The static graph and viewport use **separate Canvas layers**. Graph validation,
edge resolution, bounds and deep copying happen once in `prepareGraph()`,
not inside each viewport update. The base layer repaints only when its snapshot,
projection, palette, surface size, or device-pixel ratio changes.
Palette invalidation compares the five validated color values, not object
identity; supplying an equivalent inline palette does not repaint the graph.
Viewport-only paints do constant work independent of node/edge count.
Paints coalesce through requestAnimationFrame; cleanup cancels pending frames
and removes wheel/DPR listeners, including under React StrictMode.

`onRender` reports cumulative graph/viewport draw counts and most recent
CPU paint durations. Callback identity changes alone do not invalidate caches.
This measures Canvas command submission, not GPU completion.

The self-contained offline production demo uses **10,000 synthetic nodes and
19,801 edges**, including a routed edge. Its benchmark runs 30 warm-up frames
and 360 measured controlled pan/zoom frames, then reports:

- RAF cadence/FPS, p50/p95 intervals and viewport paint CPU time;
- actual viewport draw count and graph repaint count;
- initial graph preparation and paint time;
- browser, DPR, hardware concurrency, surface size and visibility.

The benchmark requires a visible, nonzero viewport. It rejects concurrent
runs, a hidden tab, or a two-second pause without browser frames rather than
publishing a misleading result. It measures a **static cached graph**
while the controlled viewport changes, not continuous 10k-node layout changes.
Initial preparation and graph changes have separate linear work.
RAF cadence is not a portable 60-FPS promise. Throttling, refresh rate, graph
density, Canvas implementation and hardware all affect the result.

The measured [local report](benchmarks/local-report.md) recorded baseline and
inline-palette runs at 59.967–59.973 RAF FPS, with 389–390 overlay paints and
zero graph repaints per run.
It also records the separate cold costs, environment and limitations.

## Offline development

Install pinned dev dependencies once; tests/demo do not use the network.

```sh
npm ci --ignore-scripts
npm test
npm run typecheck
npm run format:check
npm run demo:build
```

Open `dist/demo/index.html` directly as a local file. No server, CDN, fonts,
fetches, analytics or telemetry are required. Click the benchmark button and
save its JSON output for a local performance report.

Tests cover immutable data, malformed models/limits, negative and empty bounds,
routes/references, projection inversion, anchored zoom, fit, numeric clamps,
SSR, DOM-independent painter separation and public exports.
[`tests/browser-check.mjs`](tests/browser-check.mjs) is a self-contained
in-page verification script for the local demo. Run it in a visible browser
console or through local browser automation after the demo has painted.
It checks gestures, controlled rejection, disabled state, scaled anchors and
layer reuse. Run [`tests/browser-cleanup.mjs`](tests/browser-cleanup.mjs) last
to check frame/listener cleanup; it deliberately unmounts the demo.

`node examples/make-data.mjs --write` regenerates the synthetic fixture and
schema. Samples contain only invented IDs/geometry. No source identifiers,
real story text, browser profiles, absolute machine paths or engine binaries
are shipped.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
