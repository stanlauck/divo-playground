// SPDX-License-Identifier: MIT OR Apache-2.0
import { fail, record, ordinaryPrototype } from "./model.js";
import { toHex, type Hex } from "./color.js";
const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const bufferOf = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "buffer",
)!.get!;
const lengthOf = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  "length",
)!.get!;
const kindOf = Object.getOwnPropertyDescriptor(
  typedArrayPrototype,
  Symbol.toStringTag,
)!.get!;
const bufferLength = Object.getOwnPropertyDescriptor(
  ArrayBuffer.prototype,
  "byteLength",
)!.get!;
export interface RgbaImage {
  readonly width: number;
  readonly height: number;
  readonly pixels: Uint8Array | Uint8ClampedArray;
}
export const IMAGE_LIMITS = Object.freeze({
  pixels: 4_194_304,
  dimension: 4096,
  samples: 65_536,
  bytes: 8_388_608,
});
export interface CoverAccent {
  readonly color: Hex | null;
  readonly sampled: number;
  readonly visible: number;
}
export function dominantAccent(value: RgbaImage): CoverAccent {
  const r = record(value, ["width", "height", "pixels"], [], "$.image");
  const width = r.width,
    height = r.height;
  if (
    typeof width !== "number" ||
    typeof height !== "number" ||
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width > IMAGE_LIMITS.dimension ||
    height > IMAGE_LIMITS.dimension ||
    width * height > IMAGE_LIMITS.pixels
  )
    fail("dimensions", "$.image");
  const pixels = r.pixels;
  const kind = ArrayBuffer.isView(pixels) ? kindOf.call(pixels) : undefined;
  if (
    ((kind !== "Uint8Array" ||
      !ordinaryPrototype(pixels as object, Uint8Array)) &&
      (kind !== "Uint8ClampedArray" ||
        !ordinaryPrototype(pixels as object, Uint8ClampedArray))) ||
    ["buffer", "length", "byteLength", "byteOffset"].some(
      (key) => Object.getOwnPropertyDescriptor(pixels, key) !== undefined,
    )
  )
    fail("pixels", "$.image.pixels");
  try {
    bufferLength.call(bufferOf.call(pixels));
  } catch {
    fail("pixels", "$.image.pixels");
  }
  if (lengthOf.call(pixels) !== width * height * 4)
    fail("pixels", "$.image.pixels");
  const bytes = pixels as Uint8Array | Uint8ClampedArray;
  const weight = new Float64Array(32768),
    red = new Float64Array(32768),
    green = new Float64Array(32768),
    blue = new Float64Array(32768);
  const count = Math.min(width * height, IMAGE_LIMITS.samples);
  let visible = 0;
  for (let i = 0; i < count; i++) {
    const p = 4 * Math.floor(((i + 0.5) * width * height) / count),
      a = bytes[p + 3]!;
    if (a < 16) continue;
    visible++;
    const r = bytes[p]!,
      g = bytes[p + 1]!,
      b = bytes[p + 2]!,
      bin = ((r >> 3) << 10) | ((g >> 3) << 5) | (b >> 3);
    weight[bin] = weight[bin]! + a;
    red[bin] = red[bin]! + r * a;
    green[bin] = green[bin]! + g * a;
    blue[bin] = blue[bin]! + b * a;
  }
  let winner = 0;
  for (let i = 1; i < weight.length; i++)
    if (weight[i]! > weight[winner]!) winner = i;
  const color =
    weight[winner] === 0
      ? null
      : toHex({
          r: Math.round(red[winner]! / weight[winner]!),
          g: Math.round(green[winner]! / weight[winner]!),
          b: Math.round(blue[winner]! / weight[winner]!),
        });
  return Object.freeze({ color, sampled: count, visible });
}
