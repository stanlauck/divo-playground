<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# PG-11 Theme engine

Standalone TypeScript/ESM theme tokens. No framework, host application,
networking, DOM side effects or runtime dependencies in the core.

## Contract

```ts
import {
  createTheme,
  cssVariables,
  cssText,
} from "@divo-playground/pg-11-theme-engine";

const theme = createTheme({
  version: 1,
  preset: "paper", // paper | slate | forest
  mode: "light", // light | dark; no implicit OS preference
  density: "comfortable", // compact | comfortable | spacious
  accent: { source: "custom", hsl: { h: 210, s: 70, l: 44 } },
});
const variables = cssVariables(theme); // --pg11-background, --pg11-text, etc.
const stylesheet = cssText(theme); // fixed :root selector
```

`schema.json` describes JSON v1, with `samples/custom.json` as an invented
example. `parseConfig` validates plain/null-prototype data records, rejects
unknown fields, inherited data and accessors, and returns detached frozen
objects. It never copies via getters/toJSON. `ThemeError` reports a code and
structural path, not user-provided text or image filenames.

HSL hue is 0…360 degrees (360 canonicalizes to 0); saturation and lightness
are 0…100 percentages. Nonfinite and out-of-range values fail. Custom HSL
sets only the requested accent. Preset/mode derive the remaining colors.
No arbitrary CSS, alpha colors, selector input or full-token overrides.

Accent sources:

- Omit `accent` or use `{source:"preset"}` for the preset default.
- `{source:"custom",hsl:{h,s,l}}` for the requested custom accent.
- `{source:"cover"}` plus an optional RGBA image for a dominant cover accent.
  A missing or entirely transparent cover falls back to the preset.
  Invalid supplied images fail. Images supplied for non-cover sources fail,
  so unused input is not silently ignored.

`theme.accent` records source, requested color and whether correction changed
it. Config, colors, density metrics, contrast pairs and theme are frozen.

## Readability and density

Semantic colors: `background`, `surface`, `raised`, `text`, `mutedText`,
`accent`, `onAccent`, `border`, `focus`. Colors are opaque six-digit sRGB hex.
The exported HSL/RGB/hex conversions and `contrast` use the WCAG sRGB
relative-luminance transfer function.

Text, muted text and accent each meet **4.5:1** against all three base surfaces.
On-accent text meets 4.5:1 against the accent. Border/focus meet **3:1** against
the base surfaces. The engine searches the nearest passing HSL lightness
along the paths to black/white while retaining requested hue/saturation.
It measures actual rounded 8-bit hex values, not an unquantized estimate.
`theme.pairs` exposes the tested pairs, ratios and thresholds.

These guarantees cover only those opaque token pairs. They are not a full
WCAG certification and do not cover images, alpha overlays, custom consumers,
surrounding arbitrary colors, text over gradients or non-color accessibility.
Density is spacing/type/control tokens, not a layout system or touch-target
certification. Consumers must choose appropriate target sizes.

| Density     | Gap / padding | Control height | Font size | Line height | Radius |
| ----------- | ------------- | -------------- | --------- | ----------- | ------ |
| compact     | 8 px          | 28 px          | 13 px     | 1.4         | 4 px   |
| comfortable | 12 px         | 36 px          | 14 px     | 1.5         | 6 px   |
| spacious    | 16 px         | 44 px          | 16 px     | 1.5         | 8 px   |

`cssVariables`/`cssText` accept only genuine themes from this module instance.
The optional prefix matches `[a-z][a-z0-9-]{0,31}`; tokens contain only
validated colors and fixed numbers/units. Core never mutates document styles.

## Cover extraction

```ts
import {
  dominantAccent,
  createTheme,
} from "@divo-playground/pg-11-theme-engine";
const image = {
  width: 2,
  height: 1,
  pixels: new Uint8Array([48, 104, 176, 255, 48, 104, 176, 255]),
};
const accent = dominantAccent(image);
const theme = createTheme(
  {
    version: 1,
    preset: "paper",
    mode: "dark",
    density: "comfortable",
    accent: { source: "cover" },
  },
  image,
);
```

Input is packed straight-alpha RGBA8, in row-major sRGB order. Accept ordinary
`Uint8Array`/`Uint8ClampedArray` over an `ArrayBuffer`, not subclass/shared
buffers or exotic views. Dimensions 1…4096, at most 4,194,304 pixels, exact
buffer-view length width×height×4. Do not mutate input during extraction.
Core does not retain or modify pixels.

At most 65,536 evenly stratified pixel positions are sampled. Samples with
alpha below 16 are ignored. Visible samples vote by alpha in 5-bit RGB bins;
the most heavily weighted bin wins, with lowest RGB-bin index breaking ties.
The returned color is that bin's alpha-weighted channel average.
No randomness, saturation preference or locale-dependent ordering.

White/black/grayscale covers remain valid. The result reports sampled and
visible counts; transparency yields `color:null`. A dominant bin is an
approximation, not semantic cover understanding. Thin features or repeated
patterns can alias under bounded sampling.

### Browser-only local decoder

```ts
import { decodeCover } from "@divo-playground/pg-11-theme-engine/browser";
const image = await decodeCover(file); // local File/Blob, not a URL
```

The separate adapter supports PNG and baseline/extended-sequential/progressive
JPEG with a frame header within the first 256 KiB. It checks signatures and
dimensions before native decode. Maximum encoded size 8 MiB, dimensions
4096 per side and 4,194,304 decoded pixels. It then uses `createImageBitmap`,
checks decoded dimensions again, downsamples proportionally to at most 256
pixels on the long side, reads Canvas RGBA and closes the bitmap on all exits.
No network fetch, object URL, filename logging or SVG/WebP/GIF support.
Unsupported/malformed input fails safely.

Native decoders, EXIF orientation, color-profile conversion and Canvas
resampling can vary between browsers. **Determinism applies to equal RGBA
input**, not arbitrary image bytes decoded on different platforms.
Native decode is not a hardened sandbox or a cancellation/streaming API.
Browser memory includes both the bounded decoded bitmap and small Canvas.

## Offline development

```sh
npm ci --ignore-scripts
npm test
npm run typecheck
npm run format:check
npm run demo:build
```

Open `dist/demo/index.html` as a local file. It previews preset/mode/density,
custom hue and an invented cover, or decodes a user-selected local PNG/JPEG.
No assets are fetched. Do not ship real images or story data in samples.

`node examples/make-data.mjs --write` regenerates schema/sample. Tests run
offline and cover malformed input, frozen data, known color/contrast oracles,
all preset/mode/density combinations, custom accent extremes, cover weighting,
sampling, transparency, limits and safe CSS output. Browser checks are separate
from Node tests because actual decoding requires browser Canvas/ImageBitmap.
Run `tests/browser-check.mjs` in the local demo to verify native PNG/JPEG,
bounded downsampling, transparency fallback, bitmap cleanup on success/failure,
preview controls and stale asynchronous-cover protection. The example ignores
older decode results when a newer file or custom hue is selected.

## License

MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.
