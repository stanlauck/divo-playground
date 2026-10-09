// SPDX-License-Identifier: MIT OR Apache-2.0
export type Mode = "light" | "dark";
export type Preset = "paper" | "slate" | "forest";
export type Density = "compact" | "comfortable" | "spacious";
export interface Hsl {
  readonly h: number;
  readonly s: number;
  readonly l: number;
}
export type Accent = Readonly<
  { source: "preset" } | { source: "cover" } | { source: "custom"; hsl: Hsl }
>;
export interface ThemeConfig {
  readonly version: 1;
  readonly preset: Preset;
  readonly mode: Mode;
  readonly density: Density;
  readonly accent: Accent;
}
export class ThemeError extends Error {
  constructor(
    readonly code: string,
    readonly path: string,
  ) {
    super(`${code} at ${path}`);
    this.name = "ThemeError";
  }
}
export function fail(code: string, path: string): never {
  throw new ThemeError(code, path);
}
export function record(
  value: unknown,
  required: readonly string[],
  optional: readonly string[],
  path: string,
): Record<string, unknown> {
  if (
    !value ||
    typeof value !== "object" ||
    (Object.getPrototypeOf(value) !== Object.prototype &&
      Object.getPrototypeOf(value) !== null)
  )
    fail("object", path);
  const result: Record<string, unknown> = Object.create(null);
  for (const key of Reflect.ownKeys(value)) {
    if (
      typeof key !== "string" ||
      (!required.includes(key) && !optional.includes(key))
    )
      fail("fields", path);
    const d = Object.getOwnPropertyDescriptor(value, key);
    if (!d || !("value" in d) || !d.enumerable) fail("fields", path);
    result[key] = d.value as unknown;
  }
  for (const key of required) if (!(key in result)) fail("fields", path);
  return result;
}
export function validateHsl(value: unknown, path = "$.hsl"): Hsl {
  const r = record(value, ["h", "s", "l"], [], path);
  for (const key of ["h", "s", "l"] as const) {
    if (
      typeof r[key] !== "number" ||
      !Number.isFinite(r[key]) ||
      r[key] < 0 ||
      r[key] > (key === "h" ? 360 : 100)
    )
      fail("hsl", path);
  }
  return Object.freeze({
    h: (r.h as number) % 360,
    s: r.s as number,
    l: r.l as number,
  });
}
function choice<T extends string>(
  value: unknown,
  allowed: readonly T[],
  path: string,
): T {
  if (typeof value !== "string" || !allowed.includes(value as T))
    fail("enum", path);
  return value as T;
}
export function parseConfig(value: unknown): ThemeConfig {
  const r = record(
    value,
    ["version", "preset", "mode", "density"],
    ["accent"],
    "$",
  );
  if (r.version !== 1) fail("version", "$.version");
  let accent: Accent = Object.freeze({ source: "preset" });
  if ("accent" in r) {
    const a = record(r.accent, ["source"], ["hsl"], "$.accent");
    const source = choice(
      a.source,
      ["preset", "cover", "custom"],
      "$.accent.source",
    );
    if (source === "custom")
      accent = Object.freeze({
        source,
        hsl: validateHsl(a.hsl, "$.accent.hsl"),
      });
    else {
      if ("hsl" in a) fail("fields", "$.accent");
      accent = Object.freeze({ source });
    }
  }
  return Object.freeze({
    version: 1,
    preset: choice(r.preset, ["paper", "slate", "forest"], "$.preset"),
    mode: choice(r.mode, ["light", "dark"], "$.mode"),
    density: choice(
      r.density,
      ["compact", "comfortable", "spacious"],
      "$.density",
    ),
    accent,
  });
}
