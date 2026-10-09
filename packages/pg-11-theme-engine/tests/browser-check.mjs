// SPDX-License-Identifier: MIT OR Apache-2.0
// Run in the self-contained local demo. No RAF, network or real images needed.
(async () => {
  const api = window.pg11;
  if (!api) throw new Error("Demo is not ready");
  let checks = 0;
  const check = (value, label) => {
    if (!value) throw new Error(`Browser check failed: ${label}`);
    checks++;
  };
  const config = {
    version: 1,
    preset: "paper",
    mode: "light",
    density: "comfortable",
    accent: { source: "cover" },
  };
  const canvas = document.createElement("canvas");
  canvas.width = 16;
  canvas.height = 16;
  const context = canvas.getContext("2d");
  context.fillStyle = "#3068b0";
  context.fillRect(0, 0, 12, 16);
  context.fillStyle = "#dc8c30";
  context.fillRect(12, 0, 4, 16);
  const png = await new Promise((resolve) =>
    canvas.toBlob(resolve, "image/png"),
  );
  const decoded = await api.decodeCover(
    new File([png], "invented.png", { type: "image/png" }),
  );
  check(decoded.width === 16 && decoded.height === 16, "real PNG dimensions");
  const theme = api.createTheme(config, decoded);
  check(
    theme.accent.requested === "#3068b0" && theme.accent.source === "cover",
    "real PNG dominant accent",
  );
  check(
    theme.pairs.every((p) => p.ratio >= p.minimum),
    "real PNG derived contrast",
  );
  const iframe = document.createElement("iframe");
  iframe.hidden = true;
  document.body.append(iframe);
  try {
    const realm = iframe.contentWindow;
    const foreignConfig = realm.JSON.parse(JSON.stringify(config));
    const foreignImage = realm.Object.assign(new realm.Object(), {
      width: 1,
      height: 1,
      pixels: new realm.Uint8ClampedArray([48, 104, 176, 255]),
    });
    const foreignTheme = api.createTheme(foreignConfig, foreignImage);
    check(
      foreignTheme.accent.requested === "#3068b0",
      "iframe config and RGBA input",
    );
    check(
      foreignTheme.pairs.every((p) => p.ratio >= p.minimum),
      "iframe contrast",
    );
    const foreignFile = new realm.File([png], "invented-frame.png", {
      type: "image/png",
    });
    const foreignDecoded = await api.decodeCover(foreignFile);
    check(
      foreignDecoded.width === 16 && foreignDecoded.height === 16,
      "iframe File native decode",
    );
  } finally {
    iframe.remove();
  }
  const jpeg = await new Promise((resolve) =>
    canvas.toBlob(resolve, "image/jpeg", 0.95),
  );
  const jpgImage = await api.decodeCover(jpeg);
  check(jpgImage.width === 16 && jpgImage.height === 16, "real JPEG decode");
  check(
    api.createTheme(config, jpgImage).pairs.every((p) => p.ratio >= p.minimum),
    "real JPEG derived contrast",
  );
  const big = document.createElement("canvas");
  big.width = 512;
  big.height = 256;
  big.getContext("2d").fillRect(0, 0, 512, 256);
  const scaled = await api.decodeCover(
    await new Promise((resolve) => big.toBlob(resolve)),
  );
  check(
    scaled.width === 256 && scaled.height === 128,
    "proportional bounded downsample",
  );
  context.clearRect(0, 0, 16, 16);
  const transparent = await api.decodeCover(
    await new Promise((resolve) => canvas.toBlob(resolve)),
  );
  check(
    api.createTheme(config, transparent).accent.source === "fallback",
    "transparent PNG fallback",
  );
  for (const value of [
    new Blob(["not an image"]),
    new Blob([new Uint8Array(8_388_609)]),
  ]) {
    let rejected = false;
    try {
      await api.decodeCover(value);
    } catch {
      rejected = true;
    }
    check(rejected, "invalid/large input");
  }
  const bitmap = createImageBitmap;
  let closed = 0;
  try {
    window.createImageBitmap = async (blob) => {
      const image = await bitmap(blob),
        close = image.close.bind(image);
      image.close = () => {
        closed++;
        close();
      };
      return image;
    };
    await api.decodeCover(png);
    check(closed === 1, "bitmap closed after success");
    const create = document.createElement;
    try {
      document.createElement = function (name, ...args) {
        return name === "canvas"
          ? { width: 0, height: 0, getContext: () => null }
          : create.call(this, name, ...args);
      };
      let rejected = false;
      try {
        await api.decodeCover(png);
      } catch (error) {
        rejected = error.code === "canvas";
      }
      check(rejected && closed === 2, "bitmap closed after Canvas failure");
    } finally {
      document.createElement = create;
    }
  } finally {
    window.createImageBitmap = bitmap;
  }
  for (const mode of ["light", "dark"]) {
    document.getElementById("mode").value = mode;
    document.getElementById("mode").dispatchEvent(new Event("change"));
    check(window.pg11.theme.config.mode === mode, "mode preview");
    check(
      getComputedStyle(document.body).backgroundColor !== "rgba(0, 0, 0, 0)",
      "CSS variables applied",
    );
    check(
      window.pg11.theme.pairs.every((p) => p.ratio >= p.minimum),
      "preview contrast",
    );
  }
  for (const density of ["compact", "spacious"]) {
    document.getElementById("density").value = density;
    document.getElementById("density").dispatchEvent(new Event("change"));
    check(window.pg11.theme.config.density === density, "density preview");
  }
  for (const preset of ["slate", "forest"]) {
    document.getElementById("preset").value = preset;
    document.getElementById("preset").dispatchEvent(new Event("change"));
    check(window.pg11.theme.config.preset === preset, "preset preview");
  }
  const hue = document.getElementById("hue");
  hue.value = "360";
  hue.dispatchEvent(new Event("input"));
  check(window.pg11.theme.config.accent.hsl.h === 0, "custom hue preview");
  let release;
  let completed;
  let finishTimer;
  const decodedPending = new Promise((resolve) => {
    completed = resolve;
  });
  const originalBitmap = createImageBitmap;
  try {
    window.createImageBitmap = async (blob) => {
      await new Promise((resolve) => {
        release = resolve;
      });
      const image = await originalBitmap(blob);
      const close = image.close.bind(image);
      image.close = () => {
        close();
        completed();
      };
      return image;
    };
    document.getElementById("synthetic").click();
    for (let i = 0; i < 100 && !release; i++)
      await new Promise((resolve) => setTimeout(resolve, 5));
    check(typeof release === "function", "asynchronous cover begins");
    hue.value = "120";
    hue.dispatchEvent(new Event("input"));
    release();
    await Promise.race([
      decodedPending,
      new Promise(
        (_, reject) =>
          (finishTimer = setTimeout(
            () => reject(new Error("Late cover did not finish decoding")),
            2000,
          )),
      ),
    ]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    check(
      window.pg11.theme.accent.source === "custom" &&
        window.pg11.theme.config.accent.hsl.h === 120,
      "late cover cannot replace newer accent",
    );
  } finally {
    clearTimeout(finishTimer);
    window.createImageBitmap = originalBitmap;
    release?.();
  }
  return {
    result: "pass",
    checks,
    png: true,
    jpeg: true,
    downsample: true,
    bitmapClosed: true,
  };
})();
