// SPDX-License-Identifier: MIT OR Apache-2.0
import { fail, ThemeError } from "./model.js";
import { IMAGE_LIMITS, type RgbaImage } from "./palette.js";
/** Browser-only local Blob decoder. No URLs, object URLs or network access. */
export async function decodeCover(blob: Blob): Promise<RgbaImage> {
  if (typeof Blob === "undefined") fail("blob", "$.cover");
  let size: number;
  try {
    size = Object.getOwnPropertyDescriptor(Blob.prototype, "size")!.get!.call(
      blob,
    );
  } catch {
    return fail("blob", "$.cover");
  }
  if (size < 1 || size > IMAGE_LIMITS.bytes) fail("blob", "$.cover");
  let header: Uint8Array;
  try {
    const part = Blob.prototype.slice.call(blob, 0, Math.min(size, 262144));
    header = new Uint8Array(await Blob.prototype.arrayBuffer.call(part));
  } catch {
    return fail("read", "$.cover");
  }
  let width = 0,
    height = 0;
  if (
    header.length >= 33 &&
    header[0] === 137 &&
    header[1] === 80 &&
    header[2] === 78 &&
    header[3] === 71 &&
    header[4] === 13 &&
    header[5] === 10 &&
    header[6] === 26 &&
    header[7] === 10
  ) {
    const view = new DataView(header.buffer);
    if (view.getUint32(8) !== 13 || view.getUint32(12) !== 0x49484452)
      fail("image_header", "$.cover");
    width = view.getUint32(16);
    height = view.getUint32(20);
  } else if (header.length >= 4 && header[0] === 255 && header[1] === 216) {
    let offset = 2;
    while (offset + 4 <= header.length) {
      if (header[offset++] !== 255) fail("image_header", "$.cover");
      while (header[offset] === 255) offset++;
      const marker = header[offset++]!;
      if (marker === 217 || marker === 218) break;
      if (marker === 1 || (marker >= 208 && marker <= 215)) continue;
      if (offset + 2 > header.length) break;
      const length = (header[offset]! << 8) | header[offset + 1]!;
      if (length < 2 || offset + length > header.length) break;
      if ([192, 193, 194].includes(marker) && length >= 8) {
        height = (header[offset + 3]! << 8) | header[offset + 4]!;
        width = (header[offset + 5]! << 8) | header[offset + 6]!;
        break;
      }
      offset += length;
    }
  } else fail("image_format", "$.cover");
  if (
    width < 1 ||
    height < 1 ||
    width > IMAGE_LIMITS.dimension ||
    height > IMAGE_LIMITS.dimension ||
    width * height > IMAGE_LIMITS.pixels
  )
    fail("dimensions", "$.cover");
  if (
    typeof createImageBitmap !== "function" ||
    typeof document === "undefined"
  )
    fail("browser", "$.cover");
  let bitmap: ImageBitmap | undefined;
  try {
    // Native decode can apply JPEG EXIF orientation; the same area/dimension limits apply.
    bitmap = await createImageBitmap(blob);
    if (
      bitmap.width < 1 ||
      bitmap.height < 1 ||
      bitmap.width > IMAGE_LIMITS.dimension ||
      bitmap.height > IMAGE_LIMITS.dimension ||
      bitmap.width * bitmap.height > IMAGE_LIMITS.pixels
    )
      fail("dimensions", "$.cover");
    const scale = Math.min(1, 256 / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.floor(bitmap.width * scale));
    canvas.height = Math.max(1, Math.floor(bitmap.height * scale));
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) fail("canvas", "$.cover");
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    return Object.freeze({
      width: canvas.width,
      height: canvas.height,
      pixels,
    });
  } catch (error) {
    if (error instanceof ThemeError) throw error;
    return fail("decode", "$.cover");
  } finally {
    bitmap?.close();
  }
}
