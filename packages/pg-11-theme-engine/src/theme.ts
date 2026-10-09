// SPDX-License-Identifier: MIT OR Apache-2.0
import {
  parseConfig,
  fail,
  type ThemeConfig,
  type Hsl,
  type Preset,
} from "./model.js";
import { hslToHex, hexToHsl, contrast, corrected, type Hex } from "./color.js";
import { dominantAccent, type RgbaImage } from "./palette.js";
export const PRESETS = Object.freeze(["paper", "slate", "forest"] as const);
const details: Readonly<Record<Preset, { h: number; s: number; accent: Hsl }>> =
  Object.freeze({
    paper: { h: 42, s: 18, accent: { h: 210, s: 70, l: 44 } },
    slate: { h: 220, s: 12, accent: { h: 265, s: 60, l: 52 } },
    forest: { h: 145, s: 14, accent: { h: 145, s: 50, l: 36 } },
  });
const densityValues = Object.freeze({
  compact: Object.freeze({
    gap: 8,
    padding: 8,
    controlHeight: 28,
    fontSize: 13,
    lineHeight: 1.4,
    radius: 4,
  }),
  comfortable: Object.freeze({
    gap: 12,
    padding: 12,
    controlHeight: 36,
    fontSize: 14,
    lineHeight: 1.5,
    radius: 6,
  }),
  spacious: Object.freeze({
    gap: 16,
    padding: 16,
    controlHeight: 44,
    fontSize: 16,
    lineHeight: 1.5,
    radius: 8,
  }),
});
export interface Theme {
  readonly version: 1;
  readonly config: ThemeConfig;
  readonly colors: Readonly<{
    background: Hex;
    surface: Hex;
    raised: Hex;
    text: Hex;
    mutedText: Hex;
    accent: Hex;
    onAccent: Hex;
    border: Hex;
    focus: Hex;
  }>;
  readonly metrics: Readonly<{
    gap: number;
    padding: number;
    controlHeight: number;
    fontSize: number;
    lineHeight: number;
    radius: number;
  }>;
  readonly accent: Readonly<{
    source: "preset" | "custom" | "cover" | "fallback";
    requested: Hex;
    corrected: boolean;
  }>;
  readonly pairs: readonly Readonly<{
    foreground: string;
    background: string;
    ratio: number;
    minimum: number;
  }>[];
}
const themes = new WeakSet<object>();
export function createTheme(value: unknown, image?: RgbaImage): Theme {
  const config = parseConfig(value),
    p = details[config.preset],
    light = config.mode === "light";
  const background = hslToHex({ h: p.h, s: p.s, l: light ? 98 : 9 }),
    surface = hslToHex({ h: p.h, s: p.s, l: light ? 95 : 13 }),
    raised = hslToHex({ h: p.h, s: p.s, l: light ? 92 : 17 });
  const bases = [background, surface, raised];
  let requested = p.accent,
    source: Theme["accent"]["source"] = config.accent.source;
  if (config.accent.source === "custom") requested = config.accent.hsl;
  if (config.accent.source === "cover") {
    const extracted = image === undefined ? null : dominantAccent(image).color;
    if (extracted) requested = hexToHsl(extracted);
    else source = "fallback";
  } else if (image !== undefined) fail("unused_image", "$.image");
  const accent = corrected(requested, bases);
  const onAccent: Hex =
    contrast("#000000", accent) >= contrast("#ffffff", accent)
      ? "#000000"
      : "#ffffff";
  const colors = Object.freeze({
    background,
    surface,
    raised,
    text: corrected({ h: p.h, s: p.s, l: light ? 18 : 92 }, bases),
    mutedText: corrected({ h: p.h, s: p.s, l: light ? 45 : 65 }, bases),
    accent,
    onAccent,
    border: corrected({ h: p.h, s: p.s, l: light ? 70 : 35 }, bases, 3),
    focus: corrected(requested, bases, 3),
  });
  const pairs: {
    foreground: string;
    background: string;
    ratio: number;
    minimum: number;
  }[] = [];
  for (const foreground of ["text", "mutedText", "accent"] as const)
    for (const background of ["background", "surface", "raised"] as const)
      pairs.push(
        Object.freeze({
          foreground,
          background,
          ratio: contrast(colors[foreground], colors[background]),
          minimum: 4.5,
        }),
      );
  pairs.push(
    Object.freeze({
      foreground: "onAccent",
      background: "accent",
      ratio: contrast(onAccent, accent),
      minimum: 4.5,
    }),
  );
  for (const foreground of ["border", "focus"] as const)
    for (const background of ["background", "surface", "raised"] as const)
      pairs.push(
        Object.freeze({
          foreground,
          background,
          ratio: contrast(colors[foreground], colors[background]),
          minimum: 3,
        }),
      );
  const theme = Object.freeze({
    version: 1 as const,
    config,
    colors,
    metrics: densityValues[config.density],
    accent: Object.freeze({
      source,
      requested: hslToHex(requested),
      corrected: accent !== hslToHex(requested),
    }),
    pairs: Object.freeze(pairs),
  });
  themes.add(theme);
  return theme;
}
export function cssVariables(
  theme: Theme,
  prefix = "pg11",
): Readonly<Record<string, string>> {
  if (!themes.has(theme)) fail("theme", "$.theme");
  if (typeof prefix !== "string" || !/^[a-z][a-z0-9-]{0,31}$/.test(prefix))
    fail("prefix", "$.prefix");
  const result: Record<string, string> = {};
  for (const [key, value] of Object.entries(theme.colors))
    result[
      `--${prefix}-${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`
    ] = value;
  for (const [key, value] of Object.entries(theme.metrics))
    result[
      `--${prefix}-${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`
    ] = `${value}${key === "lineHeight" ? "" : "px"}`;
  return Object.freeze(result);
}
export function cssText(theme: Theme, prefix = "pg11"): string {
  return `:root {\n${Object.entries(cssVariables(theme, prefix))
    .map(([k, v]) => `  ${k}: ${v};`)
    .join("\n")}\n}\n`;
}
