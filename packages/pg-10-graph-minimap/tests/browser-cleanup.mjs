// SPDX-License-Identifier: MIT OR Apache-2.0
// Run in the local demo only AFTER interaction checks and the benchmark.
(async () => {
  const api = window.pg10;
  if (!api?.ready) throw new Error("Demo is not ready");
  const surface = document.querySelector(
    '[aria-label="Graph minimap pan and zoom"]',
  );
  const media = Object.getPrototypeOf(matchMedia("(resolution: 1dppx)"));
  const counts = {
    wheel: 0,
    windowResize: 0,
    visualResize: 0,
    media: 0,
    canceledFrames: 0,
  };
  const patches = [];
  const patch = (target, name, wrapper) => {
    const descriptor = Object.getOwnPropertyDescriptor(target, name),
      previous = target[name];
    target[name] = wrapper(previous);
    patches.push(() => {
      if (descriptor) Object.defineProperty(target, name, descriptor);
      else delete target[name];
    });
  };
  try {
    patch(
      surface,
      "removeEventListener",
      (previous) =>
        function (type, ...rest) {
          if (type === "wheel") counts.wheel++;
          return previous.call(this, type, ...rest);
        },
    );
    patch(
      window,
      "removeEventListener",
      (previous) =>
        function (type, ...rest) {
          if (type === "resize") counts.windowResize++;
          return previous.call(this, type, ...rest);
        },
    );
    if (window.visualViewport)
      patch(
        window.visualViewport,
        "removeEventListener",
        (previous) =>
          function (type, ...rest) {
            if (type === "resize") counts.visualResize++;
            return previous.call(this, type, ...rest);
          },
      );
    patch(
      media,
      "removeEventListener",
      (previous) =>
        function (type, ...rest) {
          if (type === "change") counts.media++;
          return previous.call(this, type, ...rest);
        },
    );
    patch(
      window,
      "cancelAnimationFrame",
      (previous) =>
        function (frame) {
          counts.canceledFrames++;
          return previous.call(this, frame);
        },
    );
    const before = { ...api.stats },
      callbacks = api.callbacks;
    api.unmount();
    await new Promise((resolve) =>
      requestAnimationFrame(() =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
    );
    surface.dispatchEvent(
      new WheelEvent("wheel", {
        deltaY: -100,
        bubbles: true,
        cancelable: true,
      }),
    );
    window.dispatchEvent(new Event("resize"));
    await new Promise((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(resolve)),
    );
    if (
      document.querySelectorAll("canvas").length !== 0 ||
      api.callbacks !== callbacks ||
      api.stats.graphDraws !== before.graphDraws ||
      api.stats.viewportDraws !== before.viewportDraws ||
      counts.wheel !== 1 ||
      counts.windowResize !== 2 ||
      counts.media !== 2 ||
      counts.canceledFrames < 2 ||
      (window.visualViewport && counts.visualResize !== 2)
    ) {
      throw new Error("Unmount did not clean up listeners/frames");
    }
    return { result: "pass", counts, lateCallbacks: 0, lateDraws: 0 };
  } finally {
    for (const restore of patches.reverse()) restore();
  }
})();
