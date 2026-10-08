<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Local 10k-node measurement

Measured 2026-10-09 in a fresh **visible** Chrome 154 browser on Windows x64.
The demo ran directly from its production-bundled local HTML file, without
a server or network assets. DPR 1, 32 reported logical processors, 1200×800
browser viewport, 320×200 CSS-pixel minimap.

Fixture: **10,000 synthetic nodes, 19,801 edges**, including a complete routed
edge. Graph snapshots stay fixed; React controls the viewport.

Each run used 30 warm-up and 360 measured RAF pan/zoom frames. Three consecutive
runs from the same page:

| Run | RAF FPS | p50 interval | p95 interval | Sampled overlay CPU mean | Overlay paints, including warm-up | Graph repaints |
| --- | ------- | ------------ | ------------ | ------------------------ | --------------------------------- | -------------- |
| 1   | 59.971  | 16.70 ms     | 16.80 ms     | 0.0242 ms                | 390                               | 0              |
| 2   | 59.973  | 16.70 ms     | 16.80 ms     | 0.0156 ms                | 390                               | 0              |
| 3   | 59.972  | 16.70 ms     | 16.80 ms     | 0.0186 ms                | 390                               | 0              |

The one initial preparation took **124.8 ms**; the initial static graph Canvas
paint took **7.9 ms**. These are separate cold costs, not included in steady-state
viewport paint CPU measurements. Both values describe this single page load,
not independent averages across the three runs.

This meets roughly 60-Hz local cadence for the stated **cached static graph**
scenario. It is not a promise for other hardware, refresh rates, densities,
background tabs, graph mutations or continuous layout. CPU paint durations
measure Canvas command submission, not GPU completion. Tiny durations are
subject to browser timer quantization; each measured frame samples the most
recent overlay paint.

## UI verification

- 20 in-page checks passed: zoom/buttons/keyboard, controlled rejection,
  disabled input, wheel modes, pan/cancel/lost capture, scaled wheel anchors,
  fit, actual paints and static graph layer reuse.
- Trusted native pointer capture, drag beyond the surface, release and wheel
  anchor/scroll prevention passed separately.
- DPR transitions 1.5 → 2 → 1 with viewport resize rebuilt both backing stores
  and rearmed listeners. A synthetic resize also verified the fallback when
  DPR-only browser emulation deferred media-query events.
- Unmount removed one wheel listener, two resolution listeners, two window
  resize listeners and two visual viewport resize listeners, canceled both
  pending frame handles, and produced no late draws or callbacks.
- Local axe-core 4.12.1 audit: zero reported violations; one incomplete
  contrast check for the non-text minus button. This is not a full accessibility
  certification.

## Reproduce

Run `npm ci --ignore-scripts`, `npm test`, and `npm run demo:build`. Open
`dist/demo/index.html` in a visible browser. Run `tests/browser-check.mjs`
in that page, then click **Run 360-frame benchmark** or await
`window.pg10.runBenchmark()`. Run `tests/browser-cleanup.mjs` last because
it deliberately unmounts the demo.

No browser profiles, source data, machine paths or credentials are included
in this report.
