// SPDX-License-Identifier: MIT OR Apache-2.0
import {
  createTheme,
  cssVariables,
  contrast,
  type ThemeConfig,
} from "../src/index.js";
import { decodeCover } from "../src/browser.js";
const root = document.getElementById("root")!;
root.innerHTML = `<h1>Synthetic theme engine</h1>
<label>Preset <select id="preset"><option>paper</option><option>slate</option><option>forest</option></select></label>
<label>Mode <select id="mode"><option>light</option><option>dark</option></select></label>
<label>Density <select id="density"><option>compact</option><option selected>comfortable</option><option>spacious</option></select></label>
<label>Hue <input id="hue" type="range" min="0" max="360" value="210"></label>
<button id="synthetic">Use invented cover</button>
<label>Local PNG/JPEG <input id="cover" type="file" accept="image/png,image/jpeg"></label>
<p id="status" role="status"></p>
<section class="card"><h2>Preview card</h2><p>Invented example text, never source story data.</p><p class="muted">Muted text meets the same AA target.</p><a href="#details">Accent text</a> <button class="accent">Accent button</button><input aria-label="Preview input" value="Synthetic value"></section>
<section id="details" class="raised"><p>Raised surface</p></section><pre id="report"></pre>`;
let image: Awaited<ReturnType<typeof decodeCover>> | undefined;
let source: "custom" | "cover" = "custom";
let generation = 0;
function selection(id: string): string {
  return (document.getElementById(id) as HTMLSelectElement).value;
}
function render() {
  const config: ThemeConfig = {
    version: 1,
    preset: selection("preset") as ThemeConfig["preset"],
    mode: selection("mode") as ThemeConfig["mode"],
    density: selection("density") as ThemeConfig["density"],
    accent:
      source === "cover"
        ? { source: "cover" }
        : {
            source: "custom",
            hsl: { h: Number(selection("hue")), s: 70, l: 50 },
          },
  };
  const theme = createTheme(config, source === "cover" ? image : undefined);
  for (const [key, value] of Object.entries(cssVariables(theme)))
    document.documentElement.style.setProperty(key, value);
  document.getElementById("report")!.textContent = JSON.stringify(
    theme,
    null,
    2,
  );
  Object.assign(window, {
    pg11: { theme, decodeCover, createTheme, contrast, render },
  });
}
for (const id of ["preset", "mode", "density"])
  document.getElementById(id)!.addEventListener("change", render);
document.getElementById("hue")!.addEventListener("input", () => {
  generation++;
  source = "custom";
  render();
});
async function load(blob: Blob, request: number) {
  try {
    const decoded = await decodeCover(blob);
    if (request !== generation) return;
    image = decoded;
    source = "cover";
    render();
    document.getElementById("status")!.textContent = "Local image decoded.";
  } catch {
    if (request !== generation) return;
    document.getElementById("status")!.textContent =
      "Unsupported or invalid local image.";
  }
}
document.getElementById("cover")!.addEventListener("change", () => {
  const request = ++generation;
  const file = (document.getElementById("cover") as HTMLInputElement)
    .files?.[0];
  if (file) void load(file, request);
});
document.getElementById("synthetic")!.addEventListener("click", () => {
  const request = ++generation;
  const canvas = document.createElement("canvas");
  canvas.width = 16;
  canvas.height = 16;
  const context = canvas.getContext("2d")!;
  context.fillStyle = "#3068b0";
  context.fillRect(0, 0, 12, 16);
  context.fillStyle = "#dc8c30";
  context.fillRect(12, 0, 4, 16);
  canvas.toBlob((blob) => {
    if (blob && request === generation) void load(blob, request);
  }, "image/png");
});
render();
