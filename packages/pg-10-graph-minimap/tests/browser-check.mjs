// SPDX-License-Identifier: MIT OR Apache-2.0
// Run this script in the self-contained demo page after its first Canvas paint.
(async () => {
  const api = window.pg10;
  if (!api?.ready || api.stats.graphDraws < 1)
    throw new Error("Demo has not painted");
  const surface = document.querySelector(
    '[aria-label="Graph minimap pan and zoom"]',
  );
  const zoomIn = document.querySelector('button[aria-label="Zoom in"]');
  const zoomOut = document.querySelector('button[aria-label="Zoom out"]');
  const fit = [...document.querySelectorAll("button")].find(
    (b) => b.textContent === "Fit graph",
  );
  let checks = 0;
  const check = (condition, label) => {
    if (!condition) throw new Error(`Browser check failed: ${label}`);
    checks++;
  };
  const tick = () =>
    new Promise((resolve) =>
      requestAnimationFrame(() =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
    );
  const current = () => ({ ...window.pg10.viewport });
  const key = (value) =>
    surface.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: value,
        bubbles: true,
        cancelable: true,
      }),
    );
  const original = current();
  const base = api.stats.graphDraws;
  zoomIn.click();
  await tick();
  check(current().width < original.width, "zoom in");
  zoomOut.click();
  await tick();
  check(
    Math.abs(current().width - original.width) < 1e-8,
    "zoom in/out inverse",
  );
  surface.focus();
  key("ArrowRight");
  await tick();
  check(
    current().x > original.x && current().width === original.width,
    "keyboard pan",
  );
  key("toString");
  await tick();
  check(Number.isFinite(current().x), "unknown keys ignored");
  const beforeIgnored = current(),
    callbacks = window.pg10.callbacks;
  window.pg10.setAccept(false);
  await tick();
  zoomIn.click();
  key("ArrowLeft");
  await tick();
  check(
    JSON.stringify(current()) === JSON.stringify(beforeIgnored),
    "rejected controlled changes",
  );
  check(
    window.pg10.callbacks >= callbacks + 2,
    "rejected changes still notify parent",
  );
  window.pg10.setAccept(true);
  window.pg10.setDisabled(true);
  await tick();
  const beforeDisabled = window.pg10.callbacks;
  key("+");
  zoomIn.click();
  const box = surface.getBoundingClientRect();
  surface.dispatchEvent(
    new WheelEvent("wheel", {
      deltaY: -100,
      clientX: box.left + 160,
      clientY: box.top + 100,
      bubbles: true,
      cancelable: true,
    }),
  );
  await tick();
  check(window.pg10.callbacks === beforeDisabled, "disabled ignores all input");
  check(
    getComputedStyle(surface).touchAction === "auto",
    "disabled permits native touch scrolling",
  );
  window.pg10.setDisabled(false);
  await tick();
  const beforeHorizontal = window.pg10.callbacks;
  const horizontal = new WheelEvent("wheel", {
    deltaX: 100,
    deltaY: 0,
    clientX: box.left + 160,
    clientY: box.top + 100,
    bubbles: true,
    cancelable: true,
  });
  surface.dispatchEvent(horizontal);
  await tick();
  check(
    !horizontal.defaultPrevented && window.pg10.callbacks === beforeHorizontal,
    "horizontal wheel passes through without callback",
  );
  const wheelBefore = current();
  const wheel = new WheelEvent("wheel", {
    deltaY: -100,
    clientX: box.left + 160,
    clientY: box.top + 100,
    bubbles: true,
    cancelable: true,
  });
  surface.dispatchEvent(wheel);
  await tick();
  check(
    wheel.defaultPrevented && current().width < wheelBefore.width,
    "wheel zoom prevents page scroll",
  );
  for (const deltaMode of [1, 2]) {
    const before = current().width;
    surface.dispatchEvent(
      new WheelEvent("wheel", {
        deltaY: 1,
        deltaMode,
        clientX: box.left + 160,
        clientY: box.top + 100,
        bubbles: true,
        cancelable: true,
      }),
    );
    await tick();
    check(current().width > before, "line/page wheel zoom");
  }
  // Only pointer capture is stubbed for synthetic events; real capture needs native mouse/pen input.
  const names = [
    "setPointerCapture",
    "releasePointerCapture",
    "hasPointerCapture",
  ];
  const descriptors = names.map((name) =>
    Object.getOwnPropertyDescriptor(surface, name),
  );
  let captured = null;
  surface.setPointerCapture = (id) => {
    captured = id;
  };
  surface.releasePointerCapture = () => {
    captured = null;
  };
  surface.hasPointerCapture = (id) => captured === id;
  const pointer = (type, x, y, id = 99, button = 0) =>
    surface.dispatchEvent(
      new PointerEvent(type, {
        clientX: box.left + x,
        clientY: box.top + y,
        pointerId: id,
        pointerType: "mouse",
        isPrimary: true,
        button,
        bubbles: true,
        cancelable: true,
      }),
    );
  try {
    const before = window.pg10.callbacks;
    pointer("pointerdown", 200, 130, 100, 2);
    await tick();
    check(
      window.pg10.callbacks === before && captured === null,
      "secondary button ignored",
    );
    pointer("pointerdown", 250, 150);
    await tick();
    const centered = current();
    pointer("pointermove", 270, 160);
    await tick();
    check(current().x > centered.x && current().y > centered.y, "drag pan");
    pointer("pointercancel", 270, 160);
    await tick();
    const afterCancel = current();
    pointer("pointermove", 290, 180);
    await tick();
    check(
      JSON.stringify(current()) === JSON.stringify(afterCancel) &&
        captured === null,
      "cancel stops drag",
    );
    pointer("pointerdown", 120, 80);
    await tick();
    surface.dispatchEvent(
      new PointerEvent("lostpointercapture", { bubbles: true }),
    );
    const afterLost = current();
    pointer("pointermove", 150, 80);
    await tick();
    check(
      JSON.stringify(current()) === JSON.stringify(afterLost),
      "lost capture stops drag",
    );
  } finally {
    names.forEach((name, i) => {
      if (descriptors[i]) Object.defineProperty(surface, name, descriptors[i]);
      else delete surface[name];
    });
  }
  const savedStyle = surface.getAttribute("style");
  try {
    surface.style.transform = "scale(1.5)";
    surface.style.transformOrigin = "top left";
    const scaled = surface.getBoundingClientRect();
    const wheel = new WheelEvent("wheel", {
      deltaY: -100,
      clientX: scaled.left + 300,
      clientY: scaled.top + 180,
      bubbles: true,
      cancelable: true,
    });
    const bounds = api.graph.bounds;
    const scale = Math.min(304 / bounds.width, 184 / bounds.height);
    const world = {
      x:
        (((wheel.clientX - scaled.left) * 320) / scaled.width -
          (320 - bounds.width * scale) / 2) /
          scale +
        bounds.x,
      y:
        (((wheel.clientY - scaled.top) * 200) / scaled.height -
          (200 - bounds.height * scale) / 2) /
          scale +
        bounds.y,
    };
    const before = current();
    surface.dispatchEvent(wheel);
    await tick();
    const after = current();
    for (const axis of ["x", "y"]) {
      const extent = axis === "x" ? "width" : "height";
      check(
        Math.abs(
          (world[axis] - before[axis]) / before[extent] -
            (world[axis] - after[axis]) / after[extent],
        ) < 1e-8,
        "CSS scaled wheel anchor",
      );
    }
  } finally {
    surface.setAttribute("style", savedStyle);
  }
  fit.click();
  await tick();
  const fitted = current(),
    bounds = api.graph.bounds;
  check(
    fitted.x <= bounds.x &&
      fitted.y <= bounds.y &&
      fitted.x + fitted.width >= bounds.x + bounds.width &&
      fitted.y + fitted.height >= bounds.y + bounds.height,
    "fit includes graph and route",
  );
  check(
    window.pg10.stats.graphDraws === base,
    "viewport interactions with inline palette reuse graph layer",
  );
  check(window.pg10.stats.viewportDraws > 10, "actual overlay paints");
  const bitmaps = [...document.querySelectorAll("canvas")];
  check(
    bitmaps.every((c) => c.width > 0 && c.height > 0),
    "canvas backing stores",
  );
  window.pg10.setViewport(original);
  await tick();
  const report = {
    version: 1,
    checks,
    result: "pass",
    nodes: api.graph.nodes.length,
    edges: api.graph.edges.length,
    graphDraws: window.pg10.stats.graphDraws,
    viewportDraws: window.pg10.stats.viewportDraws,
    nativePointerCapture:
      "verify separately; synthetic events stub only capture",
  };
  window.pg10.browserChecks = report;
  return report;
})();
