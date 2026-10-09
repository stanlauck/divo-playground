// SPDX-License-Identifier: MIT OR Apache-2.0
import { fail, validateHsl, type Hsl } from "./model.js";
export type Hex = `#${string}`;
export interface Rgb {
  readonly r: number;
  readonly g: number;
  readonly b: number;
}
function rgb(value: Rgb): Rgb {
  for (const n of [value.r, value.g, value.b])
    if (!Number.isInteger(n) || n < 0 || n > 255) fail("rgb", "$.rgb");
  return value;
}
export function toHex(value: Rgb): Hex {
  const { r, g, b } = rgb(value);
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}
export function fromHex(value: string): Rgb {
  if (typeof value !== "string" || !/^#[a-f\d]{6}$/i.test(value))
    fail("hex", "$.color");
  return {
    r: parseInt(value.slice(1, 3), 16),
    g: parseInt(value.slice(3, 5), 16),
    b: parseInt(value.slice(5, 7), 16),
  };
}
export function hslToHex(value: Hsl): Hex {
  const { h, s, l } = validateHsl(value);
  const saturation = s / 100,
    light = l / 100,
    chroma = (1 - Math.abs(2 * light - 1)) * saturation;
  const x = chroma * (1 - Math.abs(((h / 60) % 2) - 1)),
    m = light - chroma / 2;
  const v =
    h < 60
      ? [chroma, x, 0]
      : h < 120
        ? [x, chroma, 0]
        : h < 180
          ? [0, chroma, x]
          : h < 240
            ? [0, x, chroma]
            : h < 300
              ? [x, 0, chroma]
              : [chroma, 0, x];
  return toHex({
    r: Math.round((v[0]! + m) * 255),
    g: Math.round((v[1]! + m) * 255),
    b: Math.round((v[2]! + m) * 255),
  });
}
export function hexToHsl(value: string): Hsl {
  const v = fromHex(value),
    r = v.r / 255,
    g = v.g / 255,
    b = v.b / 255;
  const high = Math.max(r, g, b),
    low = Math.min(r, g, b),
    delta = high - low,
    l = (high + low) / 2;
  const h =
    delta === 0
      ? 0
      : 60 *
        (high === r
          ? ((g - b) / delta) % 6
          : high === g
            ? (b - r) / delta + 2
            : (r - g) / delta + 4);
  return Object.freeze({
    h: (h + 360) % 360,
    s:
      delta === 0
        ? 0
        : Math.min(100, Math.max(0, (100 * delta) / (1 - Math.abs(2 * l - 1)))),
    l: Math.min(100, Math.max(0, 100 * l)),
  });
}
export function luminance(value: string): number {
  const v = fromHex(value),
    linear = (c: number) => {
      c /= 255;
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    };
  return 0.2126 * linear(v.r) + 0.7152 * linear(v.g) + 0.0722 * linear(v.b);
}
export function contrast(a: string, b: string): number {
  const x = luminance(a),
    y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
/** Internal theme backgrounds all share a light or dark polarity. */
export function corrected(
  value: Hsl,
  backgrounds: readonly Hex[],
  minimum = 4.5,
): Hex {
  const original = hslToHex(value),
    passes = (hex: Hex) =>
      backgrounds.every((bg) => contrast(hex, bg) >= minimum);
  if (passes(original)) return original;
  const candidates: { hex: Hex; distance: number }[] = [];
  for (const end of [0, 100]) {
    const endpoint = hslToHex({ ...value, l: end });
    if (!passes(endpoint)) continue;
    let failing = value.l,
      passing = end;
    for (let i = 0; i < 24; i++) {
      const mid = (failing + passing) / 2,
        hex = hslToHex({ ...value, l: mid });
      if (passes(hex)) passing = mid;
      else failing = mid;
    }
    candidates.push({
      hex: hslToHex({ ...value, l: passing }),
      distance: Math.abs(passing - value.l),
    });
  }
  candidates.sort(
    (a, b) =>
      a.distance - b.distance || (a.hex < b.hex ? -1 : a.hex > b.hex ? 1 : 0),
  );
  if (!candidates[0]) fail("contrast", "$.theme");
  return candidates[0].hex;
}
