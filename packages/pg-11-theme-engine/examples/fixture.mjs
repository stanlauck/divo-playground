// SPDX-License-Identifier: MIT OR Apache-2.0
export const sample = {
  version: 1,
  preset: "paper",
  mode: "light",
  density: "comfortable",
  accent: { source: "custom", hsl: { h: 210, s: 70, l: 44 } },
};
export function syntheticImage(width = 16, height = 16) {
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const p = (y * width + x) * 4;
      pixels.set(
        x < width * 0.75 ? [48, 104, 176, 255] : [220, 140, 48, 255],
        p,
      );
    }
  return { width, height, pixels };
}
